#![deny(missing_docs, rustdoc::missing_crate_level_docs, unused_imports)]
#![warn(clippy::all)]
#![doc = include_str!("../README.md")]

//! lfest - leveraged futures exchange for simulated trading

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod account;
pub mod account_tracker;
mod clearing_house;
mod config;
mod contract_specification;
mod cornish_fisher;
mod exchange;
mod market_state;
mod mock_exchange;
mod order_filters;
mod order_margin;
mod position;
mod risk_engine;
#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use mock_exchange::{mock_exchange_base, mock_exchange_quote};
pub use types::Result;

/// Exports common types
pub mod prelude {
    // To make the macros work
    pub use fpdec::{self, Dec, Decimal};

    pub use crate::{
        account::Account,
        account_tracker::AccountTracker,
        base, bba,
        config::Config,
        contract_specification::*,
        exchange::Exchange,
        fee, leverage,
        market_state::MarketState,
        order_filters::{PriceFilter, QuantityFilter},
        position::Position,
        quote,
        risk_engine::RiskError,
        types::*,
    };
}
