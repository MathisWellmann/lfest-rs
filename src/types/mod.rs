mod base_currency;
mod currency_trait;
mod fee;
mod market_update;
mod quote_currency;

use std::fmt::Formatter;

pub use base_currency::*;
pub use currency_trait::Currency;
use derive_more::{Display, Into};
pub use fee::*;
pub use market_update::*;
pub use quote_currency::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
/// Side of the order
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

impl Side {
    /// Returns the inverted side
    pub fn inverted(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}
