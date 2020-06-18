extern crate pretty_env_logger;
#[macro_use] extern crate log;


pub mod exchange_decimal;
pub mod orders_decimal;
pub mod config_decimal;
mod acc_tracker_decimal;

pub mod exchange_float;
pub mod orders_float;
pub mod config_float;

pub mod contracts;
mod welford_online;
