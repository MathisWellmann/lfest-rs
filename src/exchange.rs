use getset::Getters;

use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    market_state::MarketState,
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    types::{
        Currency, Error, ExchangeOrderMeta, Filled, LimitOrder, MarginCurrency, MarketOrder,
        MarketUpdate, NewOrder, OrderError, OrderId, Pending, Result, Side, TimestampNs,
    },
};

/// The main leveraged futures exchange for simulated trading
#[derive(Debug, Clone, Getters)]
pub struct Exchange<A, Q, UserOrderId>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    /// The exchange configuration.
    #[getset(get = "pub")]
    config: Config<Q::PairedCurrency>,

    /// The current state of the simulated market.
    #[getset(get = "pub")]
    market_state: MarketState,

    /// The main user account.
    #[getset(get = "pub", get_mut = "mut")]
    account: Account<Q::PairedCurrency, UserOrderId>,

    /// A performance tracker for the user account.
    #[getset(get = "pub")]
    account_tracker: A,

    risk_engine: IsolatedMarginRiskEngine<Q::PairedCurrency>,

    clearing_house: ClearingHouse<A, Q::PairedCurrency, UserOrderId>,

    next_order_id: u64,
}

impl<A, Q, UserOrderId> Exchange<A, Q, UserOrderId>
where
    A: AccountTracker<Q::PairedCurrency>,
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone + Eq + PartialEq + std::hash::Hash + std::fmt::Debug,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<Q::PairedCurrency>) -> Self {
        let market_state = MarketState::new(config.contract_specification().price_filter.clone());
        let account = Account::new(
            config.starting_balance(),
            config.initial_leverage(),
            config.contract_specification().fee_maker,
        );
        let risk_engine = IsolatedMarginRiskEngine::<Q::PairedCurrency>::new(
            config.contract_specification().clone(),
        );
        let clearing_house = ClearingHouse::new();

        Self {
            config,
            market_state,
            clearing_house,
            risk_engine,
            account,
            account_tracker,
            next_order_id: 0,
        }
    }

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// If Ok, the executed limit orders,
    /// Some Error otherwise.
    pub fn update_state(
        &mut self,
        timestamp_ns: TimestampNs,
        market_update: MarketUpdate<Q>,
    ) -> Result<Vec<LimitOrder<Q, UserOrderId, Filled>>> {
        self.market_state
            .update_state(timestamp_ns, &market_update)?;
        self.account_tracker
            .update(timestamp_ns, &self.market_state, &self.account);
        if let Err(e) = self
            .risk_engine
            .check_maintenance_margin(&self.market_state, &self.account)
        {
            // TODO: liquidate position properly
            return Err(e.into());
        };

        let to_be_exec = self.check_resting_orders(&market_update);
        Ok(Vec::from_iter(to_be_exec.into_iter().map(|order| {
            let qty = match order.side() {
                Side::Buy => order.quantity().total(),
                Side::Sell => order.quantity().total().into_negative(),
            };
            let l_price = order.limit_price();
            self.clearing_house.settle_filled_order(
                &mut self.account,
                &mut self.account_tracker,
                qty,
                l_price,
                self.config.contract_specification().fee_maker,
                self.market_state.current_timestamp_ns(),
            );
            self.account
                .remove_executed_order_from_active(order.state().meta().id());
            self.account_tracker.log_limit_order_fill();
            order.into_filled(l_price, timestamp_ns)
        })))
    }

    /// Check if any resting orders have been executed
    fn check_resting_orders(
        &mut self,
        market_update: &MarketUpdate<Q>,
    ) -> Vec<LimitOrder<Q, UserOrderId, Pending>> {
        Vec::from_iter(
            self.account
                .active_limit_orders
                .values()
                .filter(|order| self.check_limit_order_execution(order, market_update))
                .cloned(),
        )
    }

    /// Check an individual resting order if it has been executed.
    ///
    /// # Returns:
    /// If `Some`, The order is filled and needs to be settled.
    fn check_limit_order_execution(
        &self,
        limit_order: &LimitOrder<Q, UserOrderId, Pending>,
        market_update: &MarketUpdate<Q>,
    ) -> bool {
        let limit_price = limit_order.limit_price();

        match market_update {
            MarketUpdate::Bba { .. } => {
                // Updates to the best bid and ask prices do not trigger limit orders for simulation purposes.
                false
            }
            MarketUpdate::Trade {
                price,
                quantity: _,
                side,
            } => {
                // For now we ignore the filled quantity, which will change in future versions.
                match limit_order.side() {
                    Side::Buy => *price <= limit_price && matches!(side, Side::Sell),
                    Side::Sell => *price >= limit_price && matches!(side, Side::Buy),
                }
            }
            MarketUpdate::Candle {
                bid: _,
                ask: _,
                low,
                high,
            } => match limit_order.side() {
                Side::Buy => *low < limit_price,
                Side::Sell => *high > limit_price,
            },
        }
    }

    /// Submit a new `LimitOrder` to the exchange.
    ///
    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_limit_order(
        &mut self,
        order: LimitOrder<Q, UserOrderId, NewOrder>,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending>> {
        trace!("submit_order: {:?}", order);

        // Basic checks
        self.config
            .contract_specification()
            .quantity_filter
            .validate_order_quantity(order.quantity().total())?;
        self.config
            .contract_specification()
            .price_filter
            .validate_limit_order(&order, self.market_state.mid_price())?;

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);

        match order.side() {
            Side::Buy => {
                if order.limit_price() >= self.market_state.ask() {
                    return Err(Error::OrderError(OrderError::LimitPriceAboveAsk));
                }
            }
            Side::Sell => {
                if order.limit_price() <= self.market_state.bid() {
                    return Err(Error::OrderError(OrderError::LimitPriceBelowBid));
                }
            }
        }
        self.risk_engine.check_limit_order(&self.account, &order)?;
        self.account.append_limit_order(order.clone());
        self.account_tracker.log_limit_order_submission();

        Ok(order)
    }

    /// Submit a new `MarketOrder` to the exchange.
    ///
    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_market_order(
        &mut self,
        order: MarketOrder<Q, UserOrderId, NewOrder>,
    ) -> Result<MarketOrder<Q, UserOrderId, Filled>> {
        // Basic checks
        self.config
            .contract_specification()
            .quantity_filter
            .validate_order_quantity(order.quantity().total())?;

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);

        let fill_price = match order.side() {
            Side::Buy => self.market_state.ask(),
            Side::Sell => self.market_state.bid(),
        };
        self.risk_engine
            .check_market_order(&self.account, &order, fill_price)?;
        let quantity = match order.side() {
            Side::Buy => order.quantity().total(),
            Side::Sell => order.quantity().total().into_negative(),
        };
        // From here on, everything is infallible
        self.clearing_house.settle_filled_order(
            &mut self.account,
            &mut self.account_tracker,
            quantity,
            fill_price,
            self.config.contract_specification().fee_taker,
            self.market_state.current_timestamp_ns(),
        );
        self.account_tracker.log_market_order_fill();

        Ok(order.into_filled(fill_price, self.market_state.current_timestamp_ns()))
    }

    #[inline]
    fn next_order_id(&mut self) -> OrderId {
        self.next_order_id += 1;
        self.next_order_id - 1
    }

    /// Cancel an active limit order based on the `user_order_id`.
    ///
    /// # Arguments:
    /// `user_order_id`: The user order id of the order to cancel.
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    pub fn cancel_limit_order_by_user_id(
        &mut self,
        user_order_id: UserOrderId,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending>> {
        self.account
            .cancel_order_by_user_id(user_order_id, &mut self.account_tracker)
    }

    /// Cancel an active limit order.
    ///
    /// # Arguments:
    /// `order_id`: The `id` (assigned by the exchange) of the order to cancel.
    ///
    /// # Returns:
    /// An order if successful with the given order_id.
    pub fn cancel_order(
        &mut self,
        order_id: OrderId,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending>> {
        self.account
            .cancel_limit_order(order_id, &mut self.account_tracker)
    }
}

#[cfg(test)]
mod test {
    use fpdec::Decimal;

    use crate::{mock_exchange_base, prelude::*};

    fn dummy_meta() -> ExchangeOrderMeta {
        ExchangeOrderMeta::new(0, 0)
    }

    #[test]
    fn check_limit_order_execution_buy_trade() {
        let exchange = mock_exchange_base();

        let market_update = MarketUpdate::Trade {
            price: quote!(100.0),
            quantity: base!(1.0),
            side: Side::Buy,
        };
        // Buys
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(90), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(99), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(100), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(101), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );

        // Sells
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(110), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(101), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(100), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            true
        );
    }

    #[test]
    fn check_limit_order_execution_sell_trade() {
        let exchange = mock_exchange_base();

        let market_update = MarketUpdate::Trade {
            price: quote!(100.0),
            quantity: base!(1.0),
            side: Side::Sell,
        };
        // Buys
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(90), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(99), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(100), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            true
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(101), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            true
        );

        // Sells
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(110), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(101), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(100), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
    }

    #[test]
    fn check_limit_order_execution_candle() {
        let exchange = mock_exchange_base();

        let market_update = MarketUpdate::Candle {
            bid: quote!(100),
            ask: quote!(101),
            low: quote!(98),
            high: quote!(102),
        };
        // Buys
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(90), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(98), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Buy, quote!(99), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            true
        );

        // Sells
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(110), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(102), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            false
        );
        assert_eq!(
            exchange.check_limit_order_execution(
                &LimitOrder::new(Side::Sell, quote!(101), base!(0.1))
                    .unwrap()
                    .into_pending(dummy_meta()),
                &market_update
            ),
            true
        );
    }
}
