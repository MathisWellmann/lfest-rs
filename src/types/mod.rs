mod base_currency;
mod currency_trait;
mod fee;
mod futures_type;
mod leverage;
mod market_update;
mod order;
mod quote_currency;
mod side;

pub use base_currency::BaseCurrency;
pub use currency_trait::Currency;
pub use fee::Fee;
pub use futures_type::FuturesTypes;
pub use leverage::Leverage;
pub use market_update::*;
pub use order::Order;
pub use quote_currency::QuoteCurrency;
pub use side::Side;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}
