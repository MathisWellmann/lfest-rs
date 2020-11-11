extern crate pretty_env_logger;
#[macro_use] extern crate log;


pub mod exchange_decimal;
pub mod orders_decimal;
pub mod config_decimal;
mod acc_tracker;

pub mod exchange_float;
pub mod orders_float;
pub mod config_float;

pub mod contracts;
mod welford_online;

pub use exchange_decimal::ExchangeDecimal;
pub use exchange_decimal::MarginDecimal;
pub use exchange_decimal::PositionDecimal;
pub use config_decimal::ConfigDecimal;
pub use orders_decimal::OrderDecimal;

pub use exchange_float::MarginFloat;
pub use exchange_float::PositionFloat;
pub use exchange_float::ExchangeFloat;
pub use config_float::ConfigFloat;
pub use orders_float::OrderFloat;

pub use contracts::ContractType;


/// Side of the order
#[derive(Debug, Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug)]
pub enum OrderError {
    MaxActiveOrders,
    InvalidOrder,
    InvalidPrice,
    InvalidTriggerPrice,
    InvalidOrderSize,
    NotEnoughAvailableBalance,
}

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit,
    StopMarket,
    TakeProfitLimit,
    TakeProfitMarket,
}

#[derive(Debug, Clone)]
pub enum FeeType {
    Maker,
    Taker,
}