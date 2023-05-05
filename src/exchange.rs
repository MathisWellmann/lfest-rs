use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    errors::Error,
    market_state::MarketState,
    prelude::OrderError,
    risk_engine::IsolatedMarginRiskEngine,
    types::{Currency, MarginCurrency, MarketUpdate, Order, OrderType, Side},
};

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange<A, S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    config: Config<S::PairedCurrency>,
    market_state: MarketState,
    /// The actual user of the exchange
    user_account: Account<S>,
    risk_engine: IsolatedMarginRiskEngine<S::PairedCurrency>,
    clearing_house: ClearingHouse<A, S::PairedCurrency>,
    next_order_id: u64,
}

impl<A, S> Exchange<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<S::PairedCurrency>) -> Self {
        let market_state = MarketState::new(config.price_filter().clone());
        let account = Account::new(config.starting_balance(), config.fee_taker());
        let risk_engine = IsolatedMarginRiskEngine::<S::PairedCurrency>::new(
            config.contract_specification().clone(),
        );
        let clearing_house = ClearingHouse::new(account_tracker);

        Self {
            config,
            market_state,
            clearing_house,
            risk_engine,
            user_account: account,
            next_order_id: 0,
        }
    }

    /// Return a reference to current exchange config
    #[inline(always)]
    pub fn config(&self) -> &Config<S::PairedCurrency> {
        &self.config
    }

    /// Return a reference to Account
    #[inline(always)]
    pub fn account(&self) -> &Account<S> {
        &self.user_account
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account<S> {
        &mut self.user_account
    }

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// executed orders
    /// true if position has been liquidated
    pub fn update_state(
        &mut self,
        timestamp_ns: u64,
        market_update: MarketUpdate,
    ) -> Result<(Vec<Order<S>>, bool), Error> {
        self.market_state.update_state(timestamp_ns, market_update);
        todo!("risk engine checks for liquidation");
        todo!("check for order executions");
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: Order<S>) -> Result<Order<S>, OrderError> {
        trace!("submit_order: {:?}", order);

        // Basic checks
        self.config.quantity_filter().validate_order(&order)?;
        self.config
            .price_filter()
            .validate_order(&order, self.market_state.mid_price())?;

        // assign unique order id
        order.set_id(self.next_order_id());
        order.set_timestamp(self.market_state.current_timestamp_ns());

        match order.order_type() {
            OrderType::Market => self.handle_market_order(order),
            OrderType::Limit => self.handle_new_limit_order(order),
        }
    }

    /// Check if any active orders have been triggered by the most recent price
    /// action method is called after new external data has been consumed
    fn check_orders(&mut self) {
        let keys = Vec::from_iter(
            self.user_account
                .active_limit_orders()
                .iter()
                .map(|(i, _)| *i),
        );
        for i in keys {
            self.handle_limit_order(i);
        }
    }

    fn handle_market_order(&mut self, mut order: Order<S>) -> Result<Order<S>, OrderError> {
        match order.side() {
            Side::Buy => {
                let price = self.market_state.ask();
                if self.user_account.position().size() >= S::new_zero() {
                    self.user_account
                        .try_increase_long(order.quantity(), price)
                        .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                } else {
                    if order.quantity() > self.user_account.position().size().abs() {
                        self.user_account
                            .try_turn_around_short(order.quantity(), price)
                            .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                    } else {
                        // decrease short and realize pnl.
                        self.user_account
                            .try_decrease_short(
                                order.quantity(),
                                price,
                                self.config.fee_taker(),
                                self.market_state.current_timestamp_ns(),
                            )
                            .expect("Must be valid; qed");
                    }
                }
            }
            Side::Sell => {
                let price = self.market_state.bid();
                if self.user_account.position().size() >= S::new_zero() {
                    if order.quantity() > self.user_account.position().size() {
                        self.user_account
                            .try_turn_around_long(order.quantity(), price)
                            .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                    } else {
                        // decrease_long and realize pnl.
                        self.user_account
                            .try_decrease_long(
                                order.quantity(),
                                price,
                                self.config.fee_taker(),
                                self.market_state.current_timestamp_ns(),
                            )
                            .expect("All inputs are valid; qed");
                    }
                } else {
                    self.user_account
                        .try_increase_short(order.quantity(), price)
                        .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                }
                todo!()
            }
        }
        order.mark_executed();

        Ok(order)
    }

    fn handle_new_limit_order(&mut self, order: Order<S>) -> Result<Order<S>, OrderError> {
        if self.user_account.num_active_limit_orders() >= self.config.max_num_open_orders() {
            return Err(OrderError::MaxActiveOrders);
        }
        // self.handle_limit_order(order_id);
        todo!()
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_id: u64) -> Result<(), OrderError> {
        todo!()
        // let o: Order<S> = self
        //     .user_account
        //     .active_limit_orders()
        //     .get(&order_id)
        //     .expect("This order should be in HashMap for active limit orders; qed")
        //     .clone();
        // debug!("handle_limit_order: o: {:?}", o);
        // let limit_price = o.limit_price().unwrap();
        // match o.side() {
        //     Side::Buy => {
        //         // use candle information to specify execution
        //         if self.low < limit_price {
        //             // this would be a guaranteed fill no matter the queue position in orderbook
        //             self.execute_limit(o)
        //         }
        //     }
        //     Side::Sell => {
        //         // use candle information to specify execution
        //         if self.high > limit_price {
        //             // this would be a guaranteed fill no matter the queue position in orderbook
        //             self.execute_limit(o)
        //         }
        //     }
        // }
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> u64 {
        self.next_order_id += 1;
        self.next_order_id - 1
    }
}
