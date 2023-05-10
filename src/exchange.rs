use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    market_state::MarketState,
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    types::{
        Currency, Error, MarginCurrency, MarketUpdate, Order, OrderError, OrderType, Result, Side,
    },
};

pub(crate) const EXPECT_LIMIT_PRICE: &str = "A limit price must be present for a limit order; qed";

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange<A, S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    config: Config<S::PairedCurrency>,
    market_state: MarketState,
    account: Account<S::PairedCurrency>,
    account_tracker: A,
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
        let market_state = MarketState::new(config.contract_specification().price_filter.clone());
        let account = Account::new(
            config.starting_balance(),
            config.initial_leverage(),
            config.contract_specification().fee_maker,
        );
        let risk_engine = IsolatedMarginRiskEngine::<S::PairedCurrency>::new(
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

    /// Return a reference to current exchange config
    #[inline(always)]
    pub fn config(&self) -> &Config<S::PairedCurrency> {
        &self.config
    }

    /// Return a reference to Account
    #[inline(always)]
    pub fn account(&self) -> &Account<S::PairedCurrency> {
        &self.account
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account<S::PairedCurrency> {
        &mut self.account
    }

    /// Return a reference to the `AccountTracker` for performance statistics.
    #[inline(always)]
    pub fn account_tracker(&self) -> &A {
        &self.account_tracker
    }

    /// Return a reference to the currency `MarketState`
    #[inline(always)]
    pub fn market_state(&self) -> &MarketState {
        &self.market_state
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
    ) -> Result<Vec<Order<S>>> {
        self.market_state
            .update_state(timestamp_ns, market_update)?;
        self.account_tracker.update(
            timestamp_ns,
            self.market_state.mid_price(),
            self.account
                .position
                .unrealized_pnl(self.market_state.bid(), self.market_state.ask()),
        );
        if let Err(e) = self
            .risk_engine
            .check_maintenance_margin(&self.market_state, &self.account)
        {
            todo!("liquidate position");
            return Err(e.into());
        };

        let to_be_exec = self.check_resting_orders();
        for order in to_be_exec.iter() {
            let qty = match order.side() {
                Side::Buy => order.quantity(),
                Side::Sell => order.quantity().into_negative(),
            };
            self.clearing_house.settle_filled_order(
                &mut self.account,
                &mut self.account_tracker,
                qty,
                order.limit_price().expect(EXPECT_LIMIT_PRICE),
                self.config.contract_specification().fee_maker,
                self.market_state.current_timestamp_ns(),
            );
            self.account.remove_executed_order_from_active(order.id());
        }

        Ok(to_be_exec)
    }

    /// Check if any resting orders have been executed
    fn check_resting_orders(&mut self) -> Vec<Order<S>> {
        Vec::from_iter(
            self.account
                .active_limit_orders
                .values()
                .cloned()
                .filter(|order| self.check_limit_order_execution(order)),
        )
    }

    /// Check an individual resting order if it has been executed.
    ///
    /// # Returns:
    /// If `Some`, The order is filled and needs to be settled.
    fn check_limit_order_execution(&self, order: &Order<S>) -> bool {
        let l_price = order.limit_price().expect(EXPECT_LIMIT_PRICE);
        match order.side() {
            Side::Buy => self.market_state.bid() < l_price,
            Side::Sell => self.market_state.ask() > l_price,
        }
    }

    /// Submit a new order to the exchange.
    ///
    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_order(&mut self, mut order: Order<S>) -> Result<Order<S>> {
        trace!("submit_order: {:?}", order);

        // Basic checks
        self.config
            .contract_specification()
            .quantity_filter
            .validate_order(&order)?;
        self.config
            .contract_specification()
            .price_filter
            .validate_order(&order, self.market_state.mid_price())?;

        order.set_timestamp(self.market_state.current_timestamp_ns());
        order.set_id(self.next_order_id());

        match order.order_type() {
            OrderType::Market => {
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
                order.mark_executed();
            }
            OrderType::Limit => {
                let l_price = order.limit_price().expect(EXPECT_LIMIT_PRICE);
                match order.side() {
                    Side::Buy => {
                        if l_price >= self.market_state.ask() {
                            return Err(Error::OrderError(OrderError::LimitPriceAboveAsk));
                        }
                    }
                    Side::Sell => {
                        if l_price <= self.market_state.bid() {
                            return Err(Error::OrderError(OrderError::LimitPriceBelowBid));
                        }
                    }
                }
                self.risk_engine.check_limit_order(&self.account, &order)?;
                self.account.append_limit_order(order.clone());
                self.account_tracker.log_limit_order_submission();
            }
        }

        Ok(order)
    }
    #[inline(always)]
    fn next_order_id(&mut self) -> u64 {
        self.next_order_id += 1;
        self.next_order_id - 1
    }
    /// Cancel an active order based on the user_order_id of an Order
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    pub(crate) fn cancel_order_by_user_id(
        &mut self,
        user_order_id: u64,
        account_tracker: &mut A,
    ) -> Result<Order<S>> {
        self.account
            .cancel_order_by_user_id(user_order_id, account_tracker)
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub(crate) fn cancel_order(
        &mut self,
        order_id: u64,
        account_tracker: &mut A,
    ) -> Result<Order<S>> {
        self.account.cancel_order(order_id, account_tracker)
    }
}
