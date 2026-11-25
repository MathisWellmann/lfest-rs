#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use crate::{
    order_rate_limiter::RateLimitReached,
    types::{
        MaxNumberOfActiveOrders,
        NotEnoughAvailableBalance,
        OrderId,
        OrderIdNotFound,
        OrderQuantityLTEZero,
        PriceFilterError,
        ValidateOrderNotionalError,
        ValidateOrderQuantityError,
    },
};

/// The possible errors that can occur when submitting a limit order.
#[derive(Error, Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(missing_docs, reason = "Self documenting")]
pub enum SubmitLimitOrderError {
    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error(transparent)]
    MaxNumberOfActiveLimitOrders(#[from] MaxNumberOfActiveOrders),

    #[error(transparent)]
    NotEnoughAvailableBalance(#[from] NotEnoughAvailableBalance),

    #[error(
        "The limit order `RePricing` was `GoodTillCrossing` leading to its rejection as the limit_price {limit_price} locks or crosses the away market quotation price {away_market_quotation_price}"
    )]
    GoodTillCrossingRejectedOrder {
        limit_price: String,
        away_market_quotation_price: String,
    },

    #[error(transparent)]
    PriceFilter(#[from] PriceFilterError),

    #[error(transparent)]
    ValidateOrderQuantity(#[from] ValidateOrderQuantityError),

    #[error(transparent)]
    ValidateOrderNotional(#[from] ValidateOrderNotionalError),

    #[error("Reduce-only order rejected: no opposite open position to reduce")]
    ReduceOnlyRejected,

    #[error("Immediate-or-cancel or fill-or-kill order did not cross the spread")]
    ImmediateExecutionNotMarketable,

    #[error(
        "Fill-or-kill reduce-only order rejected: the open position is smaller than the order quantity, so it could only fill partially"
    )]
    FillOrKillNotFullyFillable,

    #[error(
        "The limit order crosses the spread and would execute immediately; use `submit_limit_order_with_execution` for marketable limit orders"
    )]
    WouldExecuteImmediately,
}

#[derive(Error, Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(missing_docs, reason = "Self documenting")]
pub enum NewLimitOrderError {
    #[error("The limit price is less than or equal zero.")]
    LimitPriceLTEZero,

    #[error(transparent)]
    OrderQuantityLTEZero(#[from] OrderQuantityLTEZero),
}

#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(missing_docs, reason = "Self documenting")]
pub enum AmendLimitOrderError {
    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error("The order is no longer active")]
    OrderNoLongerActive,

    #[error("internal order id not found")]
    OrderIdNotFound {
        /// The `OrderId` that was not found.
        order_id: OrderId,
    },

    #[error(
        "The amended order quantity has already been filled in the original order. Remaining order was cancelled."
    )]
    AmendQtyAlreadyFilled,

    #[error(transparent)]
    SubmitLimitOrder(#[from] SubmitLimitOrderError),
}

#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(missing_docs, reason = "Self documenting")]
pub enum CancelLimitOrderError<UserOrderIdT> {
    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error(transparent)]
    OrderIdNotFound(#[from] OrderIdNotFound<UserOrderIdT>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_error() {
        assert_eq!(size_of::<SubmitLimitOrderError>(), 56);

        assert_eq!(size_of::<CancelLimitOrderError<()>>(), 16);
        assert_eq!(size_of::<CancelLimitOrderError<u32>>(), 16);
        assert_eq!(size_of::<CancelLimitOrderError<u64>>(), 16);
    }
}
