use std::fmt::Display;

use super::{Currency, Filled, LimitOrder, Mon, Pending, QuoteCurrency, UserOrderId};

/// Contains the possible updates to limit orders.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LimitOrderFill<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D> + Display,
    BaseOrQuote: Currency<I, D> + Display,
    UserOrderIdT: UserOrderId + Display,
{
    /// The limit order was partially filled.
    PartiallyFilled {
        /// The fill price of the event.
        fill_price: QuoteCurrency<I, D>,
        /// The quantity that was filled in the event.
        filled_quantity: BaseOrQuote,
        /// The order state after it was filled.
        order_after_fill: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    },
    /// The limit order was fully filled.
    FullyFilled {
        /// The fill price of the event.
        fill_price: QuoteCurrency<I, D>,
        /// The quantity that was filled in the event.
        filled_quantity: BaseOrQuote,
        /// The order state after it was filled.
        order_after_fill: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
    },
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> std::fmt::Display
    for LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D> + Display,
    BaseOrQuote: Currency<I, D> + Display,
    UserOrderIdT: UserOrderId + Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitOrderFill::PartiallyFilled {
                fill_price,
                filled_quantity,
                order_after_fill,
            } => write!(
                f,
                "PartiallyFilled( fill_price: {fill_price}, filled_quantity: {filled_quantity}, order_after_fill: {order_after_fill})"
            ),
            LimitOrderFill::FullyFilled {
                fill_price,
                filled_quantity,
                order_after_fill,
            } => write!(
                f,
                "FullyFilled( fill_price: {fill_price}, filled_quantity: {filled_quantity}, order_after_fill: {order_after_fill})"
            ),
        }
    }
}
