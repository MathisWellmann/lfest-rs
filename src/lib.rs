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
mod cornish_fisher;
mod exchange;
mod market_state;
mod mock_exchange;
mod order_filters;
mod order_margin;
mod position;
mod position_inner;
mod risk_engine;
#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use mock_exchange::{mock_exchange_inverse, mock_exchange_linear, MockTransactionAccounting};
pub use types::Result;

/// Exports common types
pub mod prelude {
    // To make the macros work
    pub use fpdec::{self, Dec, Decimal};

    pub use crate::{
        account_tracker::AccountTracker,
        accounting::*,
        base, bba,
        config::Config,
        contract_specification::*,
        exchange::Exchange,
        fee, leverage,
        market_state::MarketState,
        order_filters::{PriceFilter, QuantityFilter},
        position::Position,
        position_inner::PositionInner,
        quote,
        risk_engine::RiskError,
        types::*,
    };
}
