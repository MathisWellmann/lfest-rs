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
mod config;
mod cornish_fisher;
mod errors;
mod exchange;
mod limit_order_margin;
mod margin;
mod order_filters;
mod position;
mod types;
mod utils;
mod validator;

use fpdec::Decimal;

/// Exports common types
pub mod prelude {
    // To make the macros work
    pub use fpdec::{Dec, Decimal};

    pub use crate::{
        account::Account,
        account_tracker::AccountTracker,
        base, bba,
        config::Config,
        errors::{Error, OrderError, Result},
        exchange::Exchange,
        fee, leverage,
        margin::Margin,
        order_filters::{PriceFilter, QuantityFilter},
        position::Position,
        quote,
        types::*,
    };
}
