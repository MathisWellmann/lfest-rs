use std::fmt::Display;

use super::{Currency, Filled, LimitOrder, Mon, Pending, UserOrderIdT};

/// Contains the possible updates to limit orders.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
pub enum LimitOrderUpdate<I, const D: u8, BaseOrQuote, UserOrderId>
where
    I: Mon<D> + Display,
    BaseOrQuote: Currency<I, D> + Display,
    UserOrderId: UserOrderIdT + Display,
{
    /// The limit order was partially filled.
    // TODO: add the filled quantity
    PartiallyFilled(LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>),
    /// The limit order was fully filled.
    // TODO: add the filled quantity
    FullyFilled(LimitOrder<I, D, BaseOrQuote, UserOrderId, Filled<I, D, BaseOrQuote>>),
}
