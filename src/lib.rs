#![deny(missing_docs, rustdoc::missing_crate_level_docs)]
#![warn(clippy::all)]

//! lfest - leveraged futures exchange for simulated trading
//! aims to be a high performance exchange for backtesting

extern crate pretty_env_logger;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod account;
mod account_tracker;
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

use std::fmt::Formatter;

pub use account::Account;
pub use account_tracker::{AccountTracker, FullAccountTracker, NoAccountTracker, ReturnsSource};
pub use config::Config;
pub use errors::{Error, OrderError, Result};
pub use exchange::Exchange;
pub use futures_type::FuturesTypes;
pub use margin::Margin;
pub use orders::Order;
pub use position::Position;
pub use utils::round;
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

    /// Returns the inverted side
    pub fn inverted(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
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
