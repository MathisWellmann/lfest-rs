#![deny(missing_docs, rustdoc::missing_crate_level_docs)]
#![warn(clippy::all)]

//! lfest - leveraged futures exchange for simulated trading
//! aims to be a high performance exchange for backtesting

extern crate pretty_env_logger;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod acc_tracker;
mod account;
mod config;
mod errors;
mod exchange;
mod futures_type;
mod margin;
mod orders;
mod position;
mod utils;
mod validator;
mod welford_online;

pub use config::Config;
pub use errors::{Error, OrderError, Result};
pub use exchange::Exchange;
pub use futures_type::FuturesTypes;
pub use margin::Margin;
pub use orders::Order;
pub use position::Position;

pub(crate) use account::Account;
pub(crate) use utils::{max, min};
pub(crate) use validator::Validator;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// Side of the order
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Defines the two fee types for different types of orders
pub enum FeeType {
    /// The fee for passive maker orders such as limit order
    Maker,
    /// The fee for aggressive taker orders such as market and stop loss order
    Taker,
}

/// round a value to a given precision of decimal places
/// used in tests
pub fn round(val: f64, prec: i32) -> f64 {
    ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round() {
        assert_eq!(round(0.111111, 0), 0.0);
        assert_eq!(round(0.111111, 1), 0.1);
        assert_eq!(round(0.111111, 2), 0.11);
        assert_eq!(round(0.111111, 3), 0.111);
        assert_eq!(round(0.111111, 4), 0.1111);
        assert_eq!(round(0.111111, 5), 0.11111);
        assert_eq!(round(0.111111, 6), 0.111111);
    }
}
