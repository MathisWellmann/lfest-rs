#![deny(missing_docs, rustdoc::missing_crate_level_docs, unused_imports)]
#![warn(clippy::all)]
#![doc = include_str!("../README.md")]

//! lfest - leveraged futures exchange for simulated trading

#[macro_use]
extern crate serde;

pub mod account_tracker;
mod accounting;
mod config;
mod contract_specification;
mod exchange;
mod market_state;
mod market_update;
mod mock_exchange;
mod order_filters;
mod order_margin;
mod position;
mod position_inner;
mod risk_engine;
mod sample_returns_trigger;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use mock_exchange::*;
pub use types::Result;

/// Exports common types
pub mod prelude {
    pub use const_decimal;
    pub use num_traits::{One, Zero};

    pub use crate::{
        account_tracker::{AccountTracker, FullAccountTracker, NoAccountTracker},
        accounting::*,
        bba,
        config::Config,
        contract_specification::*,
        exchange::{Account, ActiveLimitOrders, Exchange},
        leverage,
        market_state::MarketState,
        market_update::*,
        order_filters::{PriceFilter, QuantityFilter},
        position::Position,
        position_inner::PositionInner,
        types::*,
    };
}
