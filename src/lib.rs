#![deny(missing_docs, rustdoc::missing_crate_level_docs, unused_dependencies)]
#![warn(clippy::all)]

//! lfest - leveraged futures exchange for simulated trading
//! aims to be a high performance exchange for backtesting

extern crate pretty_env_logger;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod account;
mod account_tracker;
mod config;
mod cornish_fisher;
mod errors;
mod exchange;
mod futures_type;
mod limit_order_margin;
mod margin;
mod orders;
mod position;
mod types;
mod utils;
mod validator;

pub use account::Account;
pub use account_tracker::{AccountTracker, FullAccountTracker, NoAccountTracker, ReturnsSource};
pub use config::Config;
pub use errors::{Error, OrderError, Result};
pub use exchange::Exchange;
pub use futures_type::FuturesTypes;
pub use margin::Margin;
pub use orders::Order;
pub use position::Position;
pub use types::*;
pub use utils::round;
pub(crate) use utils::{max, min};
pub(crate) use validator::Validator;
