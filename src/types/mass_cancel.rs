//! Structured result for bulk / mass-cancel operations (adapted from OrderBook-rs).

#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use crate::types::{
    Currency,
    LimitOrder,
    Mon,
    Pending,
    UserOrderId,
};

/// Result of a mass-cancel operation (`cancel_all_limit_orders`,
/// `cancel_limit_orders_by_side`, or portfolio-wide cancels).
///
/// Orders are returned in deterministic price-time order.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MassCancelResult<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Every resting limit order that was cancelled by this call.
    pub cancelled_orders:
        Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> MassCancelResult<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Construct a `MassCancelResult` from a pre-allocated vector of cancelled orders.
    #[inline]
    #[must_use]
    pub fn new(
        cancelled_orders: Vec<
            LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        >,
    ) -> Self {
        Self { cancelled_orders }
    }

    /// Number of orders cancelled.
    #[inline]
    #[must_use]
    pub fn cancelled_count(&self) -> usize {
        self.cancelled_orders.len()
    }

    /// `true` when zero orders were cancelled.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cancelled_orders.is_empty()
    }
}
