#![deny(missing_docs, missing_crate_level_docs)]

//! lfest - leveraged futures exchange for simulated trading
//! aims to be a high performance exchange for backtesting

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

mod acc_tracker;
mod exchange;
mod orders;
mod welford_online;
mod margin;
mod position;

pub use margin::Margin;
pub use position::Position;
pub use exchange::Exchange;
pub use orders::Order;

#[derive(Debug, Clone, Copy)]
/// Side of the order
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

#[derive(Debug)]
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

#[derive(Debug, Clone)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
    /// stop market order, will trigger a market order once the trigger price is reached
    StopMarket,
}

#[derive(Debug, Clone)]
/// Defines the two fee types for different types of orders
pub enum FeeType {
    /// The fee for passive maker orders such as limit order
    Maker,
    /// The fee for aggressive taker orders such as market and stop loss order
    Taker,
}

#[derive(Debug, Clone)]
/// Define the Exchange configuration
pub struct Config {
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    pub fee_maker: f64,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    pub fee_taker: f64,
    /// The starting balance of accounts margin denoted in BASE currency
    pub starting_balance_base: f64,
    /// set to true if you use the consume_candle() method to update external price information
    pub use_candles: bool,
    /// The leverage used for the position
    pub leverage: f64,
}

#[cfg(test)]
mod tests {
    /// round a value to a given precision of decimal places
    pub fn round(val: f64, prec: i32) -> f64 {
        ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
    }

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
