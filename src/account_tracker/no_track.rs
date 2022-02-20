use std::fmt::Display;

use crate::AccountTracker;

/// Performs no tracking of account performance
#[derive(Default, Debug, Clone)]
pub struct NoAccountTracker;

impl AccountTracker for NoAccountTracker {
    fn update(&mut self, _timestamp: u64, _price: f64, _upnl: f64) {}

    fn log_rpnl(&mut self, _rpnl: f64) {}

    fn log_fee(&mut self, _fee: f64) {}

    fn log_limit_order_submission(&mut self) {}

    fn log_limit_order_cancellation(&mut self) {}

    fn log_limit_order_fill(&mut self) {}

    fn log_trade(&mut self, _side: crate::Side, _price: f64, _size: f64) {}
}

impl Display for NoAccountTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}
