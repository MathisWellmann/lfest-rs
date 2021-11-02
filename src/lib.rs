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
mod cornish_fisher;
mod errors;
mod exchange;
mod futures_type;
mod limit_order_margin;
mod margin;
mod orders;
mod position;
mod utils;
mod validator;

pub use acc_tracker::AccTracker;
pub use acc_tracker::ReturnsSource;
pub use account::Account;
pub use config::Config;
pub use errors::{Error, OrderError, Result};
pub use exchange::Exchange;
pub use futures_type::FuturesTypes;
pub use margin::Margin;
pub use orders::Order;
pub use position::Position;

use std::fmt::Formatter;
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

impl Side {
    #[inline(always)]
    /// Return the integer representation of this enum
    pub fn as_integer(&self) -> u64 {
        match self {
            Side::Buy => 0,
            Side::Sell => 1,
        }
    }

    #[inline(always)]
    /// Parse the Side from an integer value
    pub fn from_integer(val: u64) -> Result<Self> {
        match val {
            0 => Ok(Side::Buy),
            1 => Ok(Side::Sell),
            _ => Err(Error::ParseError),
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}

impl OrderType {
    #[inline(always)]
    /// Return the integer representation of this enum
    pub fn as_integer(&self) -> u64 {
        match self {
            OrderType::Market => 0,
            OrderType::Limit => 1,
        }
    }

    #[inline(always)]
    /// Parse the OrderType from integer value
    pub fn from_integer(val: u64) -> Result<Self> {
        match val {
            0 => Ok(Self::Market),
            1 => Ok(Self::Limit),
            _ => Err(Error::ParseError),
        }
    }
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
#[inline(always)]
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
