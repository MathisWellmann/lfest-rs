use getset::Getters;

use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    market_state::MarketState,
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    types::{
        Currency, Error, ExchangeOrderMeta, Filled, LimitOrder, LimitOrderUpdate, MarginCurrency,
        MarketOrder, MarketUpdate, NewOrder, OrderError, OrderId, Pending, Result, Side,
        TimestampNs,
    },
    utils::min,
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
        let account = Account::new(config.starting_balance(), config.initial_leverage());
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
    /// If Ok, returns updates regarding limit orders, wether partially filled or fully.
    pub fn update_state(
        &mut self,
        timestamp_ns: TimestampNs,
        market_update: MarketUpdate<Q>,
    ) -> Result<Vec<LimitOrderUpdate<Q, UserOrderId>>> {
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

        match market_update {
            MarketUpdate::Bba { bid: _, ask: _ } => {
                // We don't fill orders when a new best bid and ask price is being set.
                Ok(Vec::new())
            }
            MarketUpdate::Trade {
                price,
                quantity,
                side,
            } => {
                let mut changed_orders = Vec::new();
                for mut order in self.account.active_limit_orders.clone().values().cloned() {
                    // The execution criteria.
                    if match order.side() {
                        Side::Buy => price <= order.limit_price() && matches!(side, Side::Sell),
                        Side::Sell => price >= order.limit_price() && matches!(side, Side::Buy),
                    } {
                        // Execute up to the quantity of the incoming `Trade`.

                        let filled_qty = min(quantity, order.unfilled_quantity());
                        let qty = match order.side() {
                            Side::Buy => filled_qty,
                            Side::Sell => filled_qty.into_negative(),
                        };
                        self.clearing_house.settle_filled_order(
                            &mut self.account,
                            &mut self.account_tracker,
                            qty,
                            order.limit_price(),
                            self.config.contract_specification().fee_maker,
                            self.market_state.current_timestamp_ns(),
                        );
                        // Fill order and check if it is fully filled.
                        if order.fill(order.limit_price(), filled_qty) {
                            let filled_order =
                                order.clone().into_filled(order.limit_price(), timestamp_ns);
                            changed_orders.push(LimitOrderUpdate::FullyFilled(filled_order));
                            continue;
                        }
                        changed_orders.push(LimitOrderUpdate::PartiallyFilled(order.clone()));
                    }
                }
                for update in changed_orders.iter() {
                    match update {
                        LimitOrderUpdate::FullyFilled(limit_order) => {
                            self.account
                                .remove_executed_order_from_active(limit_order.state().meta().id());
                            // TODO: we could potentially log partial fills as well...
                            self.account_tracker.log_limit_order_fill();
                        }
                        LimitOrderUpdate::PartiallyFilled(_) => {}
                    }
                }
                Ok(changed_orders)
            }
            MarketUpdate::Candle {
                bid: _,
                ask: _,
                low,
                high,
            } => {
                let mut changed_orders = Vec::new();
                // As a simplifying assumption, the order always get executed fully when using candles.
                for order in self.account.active_limit_orders.clone().values() {
                    // The execution criteria.
                    if match order.side() {
                        Side::Buy => low < order.limit_price(),
                        Side::Sell => high > order.limit_price(),
                    } {
                        // Order is executed fully with candles.
                        let qty = match order.side() {
                            Side::Buy => order.quantity(),
                            Side::Sell => order.quantity().into_negative(),
                        };
                        self.clearing_house.settle_filled_order(
                            &mut self.account,
                            &mut self.account_tracker,
                            qty,
                            order.limit_price(),
                            self.config.contract_specification().fee_maker,
                            self.market_state.current_timestamp_ns(),
                        );
                        let filled_order =
                            order.clone().into_filled(order.limit_price(), timestamp_ns);
                        changed_orders.push(LimitOrderUpdate::FullyFilled(filled_order));
                    }
                }
                for update in changed_orders.iter() {
                    match update {
                        LimitOrderUpdate::FullyFilled(limit_order) => {
                            self.account
                                .remove_executed_order_from_active(limit_order.state().meta().id());
                            self.account_tracker.log_limit_order_fill();
                        }
                        LimitOrderUpdate::PartiallyFilled(_) => {
                            panic!("Here we only get fully executed limit orders; qed")
                        }
                    }
                }
                Ok(changed_orders)
            }
        }
    }

    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_limit_order(
        &mut self,
        order: LimitOrder<Q, UserOrderId, NewOrder>,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending<Q>>> {
        trace!("submit_order: {:?}", order);

        // Basic checks
        self.config
            .contract_specification()
            .quantity_filter
            .validate_order_quantity(order.quantity())?;
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
            .validate_order_quantity(order.quantity())?;

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
            Side::Buy => order.quantity(),
            Side::Sell => order.quantity().into_negative(),
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
    ) -> Result<LimitOrder<Q, UserOrderId, Pending<Q>>> {
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
    ) -> Result<LimitOrder<Q, UserOrderId, Pending<Q>>> {
        self.account
            .cancel_limit_order(order_id, &mut self.account_tracker)
    }
}
