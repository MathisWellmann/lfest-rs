use std::fmt::Display;

use super::{
    Currency,
    Filled,
    LimitOrder,
    Mon,
    Pending,
    UserOrderId,
};

/// Contains the possible updates to limit orders.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LimitOrderFill<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D> + Display,
    BaseOrQuote: Currency<I, D> + Display,
    UserOrderIdT: UserOrderId + Display,
{
    /// The limit order was partially filled.
    /// The fill price is always the limit price.
    PartiallyFilled {
        /// The quantity that was filled in the event.
        filled_quantity: BaseOrQuote,
        /// The fee is proportional to the traded quantity and the price.
        fee: BaseOrQuote::PairedCurrency,
        /// The order state after it was filled.
        order_after_fill: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    },
    /// The limit order was fully filled.
    /// The fill price is always the limit price.
    FullyFilled {
        /// The quantity that was filled in the event.
        filled_quantity: BaseOrQuote,
        /// The fee is proportional to the traded quantity and the price.
        fee: BaseOrQuote::PairedCurrency,
        /// The order state after it was filled.
        order_after_fill: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
    },
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Display
    for LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D> + Display,
    BaseOrQuote: Currency<I, D> + Display,
    UserOrderIdT: UserOrderId + Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitOrderFill::PartiallyFilled {
                filled_quantity,
                fee,
                order_after_fill,
            } => write!(
                f,
                "PartiallyFilled( filled_quantity: {filled_quantity}, fee: {fee}, order_after_fill: {order_after_fill})"
            ),
            LimitOrderFill::FullyFilled {
                filled_quantity,
                fee,
                order_after_fill,
            } => write!(
                f,
                "FullyFilled( filled_quantity: {filled_quantity}, fee: {fee}, order_after_fill: {order_after_fill})"
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        types::BaseCurrency,
        utils::NoUserOrderId,
    };

    #[test]
    fn limit_order_fill_size() {
        assert_eq!(
            size_of::<LimitOrderFill<i32, 5, BaseCurrency<i32, 5>, NoUserOrderId>>(),
            64
        );
        assert_eq!(
            size_of::<LimitOrderFill<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>>(),
            88
        );
        assert_eq!(
            size_of::<LimitOrderFill<i32, 5, BaseCurrency<i32, 5>, i64>>(),
            72
        );
        assert_eq!(
            size_of::<LimitOrderFill<i64, 5, BaseCurrency<i64, 5>, i64>>(),
            96
        );
    }
}
