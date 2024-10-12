use super::{CurrencyMarker, Filled, LimitOrder, Mon, Pending};

/// Contains the possible updates to limit orders.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LimitOrderUpdate<T, BaseOrQuote, UserOrderId>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    UserOrderId: Clone,
{
    /// The limit order was partially filled.
    PartiallyFilled(LimitOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>),
    /// The limit order was fully filled.
    FullyFilled(LimitOrder<T, BaseOrQuote, UserOrderId, Filled<T, BaseOrQuote>>),
}
