use std::fmt::Display;

use crate::{
    account_tracker::AccountTracker,
    prelude::MarketState,
    types::{Currency, MarginCurrency, QuoteCurrency, Side, TimestampNs},
};

/// Performs no tracking of account performance
#[derive(Default, Debug, Clone)]
pub struct NoAccountTracker;

impl<M> AccountTracker<M> for NoAccountTracker
where
    M: Currency + MarginCurrency,
{
    fn update(&mut self, _timestamp: TimestampNs, _market_state: &MarketState, _upnl: M) {}

    fn log_rpnl(&mut self, _rpnl: M, _ts_ns: i64) {}

    fn log_fee(&mut self, _fee: M) {}

    fn log_limit_order_submission(&mut self) {}

    fn log_limit_order_cancellation(&mut self) {}

    fn log_limit_order_fill(&mut self) {}

    fn log_market_order_fill(&mut self) {}

    fn log_trade(&mut self, _side: Side, _price: QuoteCurrency, _size: M::PairedCurrency) {}
}

impl Display for NoAccountTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}
