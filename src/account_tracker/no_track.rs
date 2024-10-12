use std::fmt::Display;

use crate::{
    account_tracker::AccountTracker,
    prelude::{MarketState, Mon, Monies, Quote, Side},
    types::MarginCurrencyMarker,
};

/// Performs no tracking of account performance
#[derive(Default, Debug, Clone)]
pub struct NoAccountTracker;

impl<T, BaseOrQuote> AccountTracker<T, BaseOrQuote> for NoAccountTracker
where
    T: Mon,
    BaseOrQuote: MarginCurrencyMarker<T>,
{
    fn update(&mut self, _market_state: &MarketState<T>) {}

    fn sample_user_balances(
        &mut self,
        _user_balances: &crate::prelude::UserBalances<T, BaseOrQuote>,
        _mid_price: Monies<T, Quote>,
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
        _price: Monies<T, Quote>,
        _quantity: Monies<T, BaseOrQuote::PairedCurrency>,
    ) {
    }
}

impl Display for NoAccountTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}
