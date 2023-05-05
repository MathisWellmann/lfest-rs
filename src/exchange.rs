use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    errors::Error,
    execution_engine::ExecutionEngine,
    market_state::MarketState,
    matching_engine::MatchingEngine,
    prelude::OrderError,
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    types::{Currency, MarginCurrency, MarketUpdate, Order, OrderType},
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
    user_account: Account<S>,
    risk_engine: IsolatedMarginRiskEngine<S::PairedCurrency>,
    matching_engine: MatchingEngine<S>,
    execution_engine: ExecutionEngine<S>,
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
            matching_engine: MatchingEngine::default(),
            execution_engine: ExecutionEngine::default(),
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
        self.market_state
            .update_state(timestamp_ns, market_update)?;
        if let Err(e) = self
            .risk_engine
            .check_maintenance_margin(&self.market_state, &self.user_account)
        {
            todo!("liquidate position");
            return Err(e.into());
        };

        let exec_orders = self
            .matching_engine
            .handle_resting_orders(&self.market_state);

        todo!("handle order executions");
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
            OrderType::Market => {
                todo!("risk engine checks");
                self.execution_engine.execute_market_order(order);
            }
            OrderType::Limit => {
                todo!("risk engine checks");
                todo!("If passing, place into orderbook of matching engine");
            }
        }
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> u64 {
        self.next_order_id += 1;
        self.next_order_id - 1
    }
}
