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

pub use exchange_float::MarginFloat;
pub use exchange_float::PositionFloat;
pub use exchange_float::ExchangeFloat;
pub use exchange_decimal::ExchangeDecimal;
pub use exchange_decimal::Margin as MarginDecimal;
pub use exchange_decimal::Position as PositionDecimal;
pub use contracts::ContractType;
pub use config_float::Config as ConfigFloat;
pub use config_decimal::Config as ConfigDecimal;
pub use orders_decimal::Order as OrderDecimal;
pub use orders_float::OrderFloat;

