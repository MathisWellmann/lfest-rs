#![doc = include_str!("../README.md")]
#![feature(vec_push_within_capacity, cold_path)]

//! lfest - leveraged futures exchange for simulated trading

mod account;
mod config;
mod contract_specification;
mod exchange;
mod expect_messages;
mod load_trades_from_csv;
mod market_state;
mod market_update;
mod mock_exchange;
mod order_filters;
pub mod order_rate_limiter;
mod risk_engine;
#[cfg(test)]
mod tests;
#[cfg(feature = "trade_aggregation")]
mod trade_aggregation;
mod types;
mod utils;

pub use expect_messages::*;
pub use load_trades_from_csv::load_trades_from_csv;
pub use mock_exchange::*;

/// Exports common types
pub mod prelude {
    pub use const_decimal;
    pub use num_traits::{
        One,
        Zero,
    };

    pub use crate::{
        account::*,
        config::Config,
        contract_specification::*,
        exchange::Exchange,
        leverage,
        market_state::MarketState,
        market_update::*,
        order_filters::{
            PriceFilter,
            QuantityFilter,
        },
        types::*,
        utils::{
            NoUserOrderId,
            decimal_from_f64,
            scale,
        },
    };
}
