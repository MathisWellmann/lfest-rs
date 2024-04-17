mod currency;
mod errors;
mod fee;
mod leverage;
mod market_update;
mod order;
mod order_type;
mod side;

pub use currency::{BaseCurrency, Currency, MarginCurrency, QuoteCurrency};
pub use errors::*;
pub use fee::{Fee, FeeType};
pub use leverage::Leverage;
pub use market_update::MarketUpdate;
pub use order::{Filled, Order};
pub use order_type::OrderType;
pub use side::Side;

/// Natural Logarithmic Returns newtype wrapping a borrowed slice of generic floats.
pub struct LnReturns<'a, T: num_traits::Float>(pub &'a [T]);
