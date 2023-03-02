#![deny(missing_docs, rustdoc::missing_crate_level_docs, unused_imports)]
#![warn(clippy::all)]

//! lfest - leveraged futures exchange for simulated trading
//! aims to be a high performance exchange for backtesting

extern crate pretty_env_logger;
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
mod futures_type;
mod limit_order_margin;
mod margin;
mod order_filters;
mod orders;
mod position;
mod types;
mod utils;
mod validator;

pub(crate) use utils::{max, min};
pub(crate) use validator::Validator;

/// Exports common types
pub mod prelude {
    pub use crate::{
        account::Account,
        account_tracker::AccountTracker,
        config::Config,
        errors::{Error, OrderError, Result},
        exchange::Exchange,
        futures_type::FuturesTypes,
        margin::Margin,
        order_filters::{PriceFilter, QuantityFilter},
        orders::Order,
        position::Position,
        types::*,
    };
}
