mod currency;
mod fee;
mod futures_type;
mod leverage;
mod market_update;
mod order;
mod side;

pub use currency::{BaseCurrency, Currency, QuoteCurrency};
pub use fee::Fee;
pub use futures_type::FuturesTypes;
pub use leverage::Leverage;
pub use market_update::*;
pub use order::Order;
pub use side::Side;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}
