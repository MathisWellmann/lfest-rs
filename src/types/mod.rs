mod currency;
mod errors;
mod fee;
mod leverage;
mod limit_order;
mod market_order;
mod market_update;
mod order_meta;
mod order_status;
mod order_update;
mod side;

pub use currency::{BaseCurrency, Currency, MarginCurrency, QuoteCurrency};
pub use errors::*;
pub use fee::{Fee, FeeType};
pub use leverage::Leverage;
pub use limit_order::LimitOrder;
pub use market_order::MarketOrder;
pub use market_update::MarketUpdate;
pub use order_meta::ExchangeOrderMeta;
pub use order_status::{Filled, FilledQuantity, NewOrder, Pending};
pub use order_update::LimitOrderUpdate;
pub use side::Side;

/// Natural Logarithmic Returns newtype wrapping a borrowed slice of generic floats.
pub struct LnReturns<'a, T: num_traits::Float>(pub &'a [T]);

/// The type for the global order id sequence number used by the exchange.
pub type OrderId = u64;

/// The type of a timestamp that is measured in nanoseconds.
pub type TimestampNs = i64;
