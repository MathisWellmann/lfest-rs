mod config;
mod filter;
mod main;
mod order;
mod risk;

pub use config::ConfigError;
pub use filter::FilterError;
pub use main::Error;
pub use order::*;
pub use risk::RiskError;

/// This is defined as a convenience.
pub type Result<T, E = Error> = std::result::Result<T, E>;
