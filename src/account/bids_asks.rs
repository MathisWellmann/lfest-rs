use std::cmp::Ordering;

use crate::types::{
    Currency,
    LimitOrder,
    Mon,
    Pending,
    Side,
    UserOrderId,
};

/// zero-sized marker struct indicating sorting for bids.
#[derive(Debug, PartialEq, Eq)]
pub struct Bids;

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Cmp<I, D, BaseOrQuote, UserOrderIdT> for Bids
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[inline(always)]
    fn is_same_side(side: Side) -> bool {
        side == Side::Buy
    }

    /// New orders which have a higher price will come later in the vector.
    /// Older orders at the same price level come later in the vector.
    #[inline(always)]
    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering {
        use Ordering::*;
        match new_order.limit_price().cmp(&existing_order.limit_price()) {
            Less => Less,
            Equal => {
                match new_order
                    .state()
                    .meta()
                    .ts_exchange_received()
                    .cmp(&existing_order.state().meta().ts_exchange_received())
                {
                    Less => Greater, // Older orders should be later.
                    Equal => Equal,
                    Greater => Less,
                }
            }
            Greater => Greater,
        }
    }
}

/// zero-sized marker struct indicating sorting for asks.
#[derive(Debug, PartialEq, Eq)]
pub struct Asks;

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Cmp<I, D, BaseOrQuote, UserOrderIdT> for Asks
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[inline(always)]
    fn is_same_side(side: Side) -> bool {
        side == Side::Sell
    }

    /// New orders which have a lower price will come later in the vector.
    /// Older orders at the same price level come later in the vector.
    #[inline(always)]
    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering {
        use Ordering::*;
        match new_order.limit_price().cmp(&existing_order.limit_price()) {
            Less => Greater,
            Equal => {
                match new_order
                    .state()
                    .meta()
                    .ts_exchange_received()
                    .cmp(&existing_order.state().meta().ts_exchange_received())
                {
                    Less => Greater,
                    Equal => Equal,
                    Greater => Less,
                }
            }
            Greater => Less,
        }
    }
}

/// Provides the sorting between two limit orders.
/// This differs for bids and asks.
pub trait Cmp<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// If `true`, the side is the same as the marker struct implementing this function.
    /// Used for asserting that only limit orders of the correct side are included.
    fn is_same_side(side: Side) -> bool;

    /// Compare a new order with an existing one for ordering them appropriately.
    /// This implementation differs for `Bids` and `Asks` as the best price and oldest order is always at the last vector position.
    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering;
}
