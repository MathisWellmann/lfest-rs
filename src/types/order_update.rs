use super::{CurrencyMarker, Filled, LimitOrder, Mon, Pending};

/// Contains the possible updates to limit orders.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LimitOrderUpdate<I, const DB: u8, const DQ: u8, BaseOrQuote, UserOrderId>
where
    I: Mon<DQ> + Mon<DB>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
    UserOrderId: Clone,
{
    /// The limit order was partially filled.
    PartiallyFilled(
        LimitOrder<I, DB, DQ, BaseOrQuote, UserOrderId, Pending<I, DB, DQ, BaseOrQuote>>,
    ),
    /// The limit order was fully filled.
    FullyFilled(LimitOrder<I, DB, DQ, BaseOrQuote, UserOrderId, Filled<I, DB, DQ, BaseOrQuote>>),
}
