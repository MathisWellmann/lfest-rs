#![deny(missing_docs, missing_crate_level_docs)]

//! lfest - leveraged futures exchange for simulated trading
//! aims to be a high performance exchange for backtesting

extern crate pretty_env_logger;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod acc_tracker;
mod account;
mod exchange;
mod futures_type;
mod margin;
mod orders;
mod position;
mod utils;
mod validator;
mod welford_online;

pub use exchange::Exchange;
pub use futures_type::FuturesType;
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

#[derive(Debug, Serialize, Deserialize)]
/// Defines the possible order errors that can occur when submitting a new order
pub enum OrderError {
    /// Maximum number of active orders reached
    MaxActiveOrders,
    /// Invalid limit price of order
    InvalidLimitPrice,
    /// Invalid trigger price for order. e.g.: sell stop market order trigger price > ask
    InvalidTriggerPrice,
    /// Invalid order size
    InvalidOrderSize,
    /// The account does not have enough available balance to submit the order
    NotEnoughAvailableBalance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
    /// stop market order, will trigger a market order once the trigger price is reached
    StopMarket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Defines the two fee types for different types of orders
pub enum FeeType {
    /// The fee for passive maker orders such as limit order
    Maker,
    /// The fee for aggressive taker orders such as market and stop loss order
    Taker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Define the Exchange configuration
pub struct Config {
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    pub fee_maker: f64,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    pub fee_taker: f64,
    /// The starting balance of account
    pub starting_balance: f64,
    /// set to true if you use the consume_candle() method to update external price information
    pub use_candles: bool,
    /// The leverage used for the position
    pub leverage: f64,
    /// The type of futures to simulate
    pub futures_type: FuturesType,
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
