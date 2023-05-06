use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    execution_engine::ExecutionEngine,
    market_state::MarketState,
    matching_engine::MatchingEngine,
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    types::{Currency, MarginCurrency, MarketUpdate, Order, OrderType, Result, Side},
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
    user_account: Account<S::PairedCurrency>,
    risk_engine: IsolatedMarginRiskEngine<S::PairedCurrency>,
    matching_engine: MatchingEngine<S>,
    execution_engine: ExecutionEngine<A, S>,
    clearing_house: ClearingHouse<A, S::PairedCurrency>,
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
        let account = Account::new(config.starting_balance(), config.initial_leverage());
        let risk_engine = IsolatedMarginRiskEngine::<S::PairedCurrency>::new(
            config.contract_specification().clone(),
        );
        let clearing_house = ClearingHouse::new(account_tracker);
        let execution_engine = ExecutionEngine::<A, S>::new();

        Self {
            config,
            market_state,
            clearing_house,
            risk_engine,
            matching_engine: MatchingEngine::default(),
            execution_engine,
            user_account: account,
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
        &self.user_account
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account<S::PairedCurrency> {
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
    ) -> Result<Vec<Order<S>>> {
        self.market_state
            .update_state(timestamp_ns, market_update)?;
        if let Err(e) = self
            .risk_engine
            .check_maintenance_margin(&self.market_state, &self.user_account)
        {
            todo!("liquidate position");
            return Err(e.into());
        };

        // TODO:
        // let exec_orders = self
        //     .matching_engine
        //     .handle_resting_orders(&self.market_state);

        // todo!("handle order executions");
        Ok(vec![])
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

        match order.order_type() {
            OrderType::Market => {
                let fill_price = match order.side() {
                    Side::Buy => self.market_state.ask(),
                    Side::Sell => self.market_state.bid(),
                };
                let req_margin = self.risk_engine.check_required_margin(
                    &self.user_account,
                    &order,
                    fill_price,
                )?;
                let quantity = match order.side() {
                    Side::Buy => order.quantity(),
                    Side::Sell => order.quantity().into_negative(),
                };
                // From here on, everything is infallible
                self.clearing_house.settle_filled_order(
                    &mut self.user_account,
                    quantity,
                    fill_price,
                    req_margin,
                );
                order.mark_executed();

                Ok(order)
            }
            OrderType::Limit => {
                todo!("risk engine checks");
                todo!("If passing, place into orderbook of matching engine");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use fpdec::Decimal;

    use crate::{mock_exchange_base, prelude::*};

    #[test]
    fn submit_order() {
        let mut exchange = mock_exchange_base();
        assert_eq!(
            exchange
                .update_state(0, bba!(quote!(100), quote!(101)))
                .unwrap(),
            vec![]
        );

        let order = Order::market(Side::Buy, base!(5)).unwrap();
        exchange.submit_order(order).unwrap();

        // make sure its excuted immediately
        assert_eq!(
            exchange.account().position,
            Position {
                size: base!(5),
                entry_price: quote!(101),
                position_margin: quote!(50.5),
            }
        );

        // TODO: test more
        todo!()
    }
}
