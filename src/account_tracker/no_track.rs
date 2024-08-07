use std::fmt::Display;

use crate::{
    account_tracker::AccountTracker,
    prelude::{MarketState, QuoteCurrency, Side},
    types::{Currency, MarginCurrency},
};

/// Performs no tracking of account performance
#[derive(Default, Debug, Clone)]
pub struct NoAccountTracker;

impl<M> AccountTracker<M> for NoAccountTracker
where
    M: Currency + MarginCurrency,
{
    fn update(&mut self, _market_state: &MarketState) {}

    fn sample_user_balances(
        &mut self,
        _user_balances: &crate::prelude::UserBalances<M>,
        _mid_price: QuoteCurrency,
    ) {
    }

    fn log_limit_order_submission(&mut self) {}

    fn log_limit_order_cancellation(&mut self) {}

    fn log_limit_order_fill(&mut self) {}

    fn log_market_order_submission(&mut self) {}

    fn log_market_order_fill(&mut self) {}

    fn log_trade(
        &mut self,
        _side: Side,
        _price: QuoteCurrency,
        _quantity: <M as Currency>::PairedCurrency,
    ) {
    }
}

impl Display for NoAccountTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}
