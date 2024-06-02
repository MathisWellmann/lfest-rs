use super::{Currency, Filled, LimitOrder, Pending};

/// Contains the possible updates to limit orders.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LimitOrderUpdate<Q, UserOrderId>
where
    Q: Currency,
    UserOrderId: Clone,
{
    /// The limit order was partially filled.
    PartiallyFilled(LimitOrder<Q, UserOrderId, Pending<Q>>),
    /// The limit order was fully filled.
    FullyFilled(LimitOrder<Q, UserOrderId, Filled<Q>>),
}
