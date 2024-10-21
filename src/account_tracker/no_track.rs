use std::fmt::Display;

use crate::{
    account_tracker::AccountTracker,
    prelude::{MarketState, Mon, QuoteCurrency, Side, UserBalances},
    types::MarginCurrency,
};

/// Performs no tracking of account performance
#[derive(Default, Debug, Clone)]
pub struct NoAccountTracker;

impl<I, const D: u8, BaseOrQuote> AccountTracker<I, D, BaseOrQuote> for NoAccountTracker
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    fn update(&mut self, _market_state: &MarketState<I, D>) {}

    fn sample_user_balances(
        &mut self,
        _user_balances: &UserBalances<BaseOrQuote>,
        _mid_price: QuoteCurrency<I, D>,
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
        _price: QuoteCurrency<I, D>,
        _quantity: BaseOrQuote::PairedCurrency,
    ) {
    }
}

impl Display for NoAccountTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}
