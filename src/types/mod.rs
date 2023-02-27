mod base_currency;
mod currency_trait;
mod fee;
mod leverage;
mod market_update;
mod quote_currency;
mod side;

pub use base_currency::*;
pub use currency_trait::Currency;
pub use fee::Fee;
pub use leverage::Leverage;
pub use market_update::*;
pub use quote_currency::*;
pub use side::Side;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}
