#![doc = include_str!("../README.md")]
#![feature(vec_push_within_capacity)]

//! lfest - leveraged futures exchange for simulated trading

mod account;
mod config;
mod contract_specification;
mod exchange;
mod expect_messages;
pub mod fxmacrodata;
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
    // Re-export every `num_traits` trait that appears in a public supertrait
    // bound (`Mon`, `Currency`) or is implemented on a public currency type.
    // Trait methods are only callable when the trait is in scope, so omitting
    // one (e.g. `Signed` for `abs()`, `is_positive()`, `signum()`) causes
    // "method not found" (E0599) for downstream users despite the impl
    // existing.
    pub use num_traits::{
        Num,
        One,
        Signed,
        Zero,
    };

    pub use crate::{
        account::*,
        config::Config,
        contract_specification::*,
        exchange::{
            Exchange,
            ForcedCancels,
            MarketOrderSettlement,
        },
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
