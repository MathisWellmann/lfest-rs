mod errors;
mod fee;
mod leverage;
mod limit_order;
mod market_order;
mod order_id;
mod order_meta;
mod order_status;
mod order_update;
mod re_pricing;
mod side;
mod smol_currency;
mod timestamp_ns;

pub use errors::*;
pub use fee::{Fee, Maker, Taker};
pub use leverage::Leverage;
pub use limit_order::LimitOrder;
pub use market_order::MarketOrder;
pub use order_id::OrderId;
pub use order_meta::ExchangeOrderMeta;
pub use order_status::{Filled, FilledQuantity, NewOrder, Pending};
pub use order_update::LimitOrderUpdate;
pub use re_pricing::RePricing;
pub use side::Side;
pub use smol_currency::{BaseCurrency, Currency, MarginCurrency, Mon, QuoteCurrency};
pub use timestamp_ns::TimestampNs;

/// Natural Logarithmic Returns newtype wrapping a borrowed slice of generic floats.
pub struct LnReturns<'a, T: num_traits::Float>(pub &'a [T]);

/// The user balances denoted in the margin currency.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UserBalances<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// The available wallet balance that is used to provide margin for positions and orders.
    pub available_wallet_balance: BaseOrQuote,
    /// The margin reserved for the position.
    pub position_margin: BaseOrQuote,
    /// The margin reserved for the open limit orders.
    pub order_margin: BaseOrQuote,
    /// Just a marker type.
    pub _q: std::marker::PhantomData<QuoteCurrency<I, D>>,
}

impl<I, const D: u8, BaseOrQuote> UserBalances<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Sum of all balances.
    pub fn sum(&self) -> BaseOrQuote {
        self.available_wallet_balance + self.position_margin + self.order_margin
    }
}

/// A custom user order id must satisfy this trait bound.
pub trait UserOrderIdT:
    Clone + Copy + Eq + PartialEq + std::hash::Hash + std::fmt::Debug + Default
{
}

// Blanket impl
impl<T> UserOrderIdT for T where
    T: Clone + Copy + Eq + PartialEq + std::hash::Hash + std::fmt::Debug + Default
{
}
