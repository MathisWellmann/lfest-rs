mod errors;
mod fee;
mod leverage;
mod limit_order;
mod limits;
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
pub use fee::{
    Fee,
    Maker,
    Taker,
};
pub use leverage::Leverage;
pub use limit_order::LimitOrder;
pub use limits::OrderRateLimits;
pub use market_order::MarketOrder;
pub use order_id::OrderId;
pub use order_meta::ExchangeOrderMeta;
pub use order_status::{
    Filled,
    FilledQuantity,
    NewOrder,
    Pending,
};
pub use order_update::LimitOrderFill;
pub use re_pricing::RePricing;
pub use side::Side;
pub use smol_currency::{
    BaseCurrency,
    Currency,
    MarginCurrency,
    Mon,
    QuoteCurrency,
};
pub(crate) use timestamp_ns::NANOS_PER_SECOND;
pub use timestamp_ns::TimestampNs;

/// Natural Logarithmic Returns newtype wrapping a borrowed slice of generic floats.
pub struct LnReturns<'a, T: num_traits::Float>(pub &'a [T]);

/// A custom user order id must satisfy this trait bound.
pub trait UserOrderId:
    Clone + Copy + Eq + PartialEq + std::fmt::Debug + std::fmt::Display + Default
{
}

// Blanket impl
impl<T> UserOrderId for T where
    T: Clone + Copy + Eq + PartialEq + std::fmt::Debug + std::fmt::Display + Default
{
}

/// Whether to cancel a limit order by its `OrderId` or the `UserOrderId`.
#[allow(missing_docs, reason = "Self documenting")]
#[derive(Debug, Clone, Copy)]
pub enum CancelBy<UserOrderIdT: UserOrderId> {
    OrderId(OrderId),
    UserOrderId(UserOrderIdT),
}
