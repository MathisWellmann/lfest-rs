// TODO: Split this into more files.

use std::fmt::Display;

use thiserror::Error;

use crate::{
    order_rate_limiter::RateLimitReached,
    types::{
        NotEnoughAvailableBalance,
        OrderId,
        PriceFilterError,
    },
};

/// The possible errors that can occur when submitting a limit order.
#[derive(Error, Debug, Clone, Eq, PartialEq)]
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
}

#[derive(Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum NewLimitOrderError {
    #[error("The limit price is less than or equal zero.")]
    LimitPriceLTEZero,

    #[error(transparent)]
    OrderQuantityLTEZero(#[from] OrderQuantityLTEZero),
}

#[derive(Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub struct OrderQuantityLTEZero;

impl Display for OrderQuantityLTEZero {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "order quantity is less than or equal to zero.")
    }
}

#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum ValidateOrderQuantityError {
    #[error("order size is less than or equal zero.")]
    OrderQuantityLTEZero,

    #[error("The order quantity is too low")]
    QuantityTooLow,

    #[error("The order quantity is too high")]
    QuantityTooHigh,

    #[error("The order quantity does not conform to the step size")]
    InvalidQuantityStepSize,
}

#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum SubmitMarketOrderError {
    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error(transparent)]
    NotEnoughAvailableBalance(#[from] NotEnoughAvailableBalance),

    #[error(transparent)]
    ValidateOrderQuantity(#[from] ValidateOrderQuantityError),
}

/// zero-sized error variant
#[derive(Error, Debug, Clone, derive_more::Display, Eq, PartialEq)]
pub struct MaxNumberOfActiveOrders;

#[derive(Debug, Clone, Error, Eq, PartialEq)]
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
#[allow(missing_docs, reason = "Self documenting")]
pub enum CancelLimitOrderError<UserOrderIdT> {
    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error(transparent)]
    OrderIdNotFound(#[from] OrderIdNotFound<UserOrderIdT>),
}

#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum OrderIdNotFound<UserOrderIdT> {
    OrderId(OrderId),
    UserOrderId(UserOrderIdT),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_error() {
        assert_eq!(size_of::<MaxNumberOfActiveOrders>(), 0);
        assert_eq!(size_of::<SubmitLimitOrderError>(), 56);

        assert_eq!(size_of::<OrderIdNotFound<()>>(), 16);
        assert_eq!(size_of::<OrderIdNotFound<u32>>(), 16);
        assert_eq!(size_of::<OrderIdNotFound<u64>>(), 16);

        assert_eq!(size_of::<CancelLimitOrderError<()>>(), 16);
        assert_eq!(size_of::<CancelLimitOrderError<u32>>(), 16);
        assert_eq!(size_of::<CancelLimitOrderError<u64>>(), 16);
    }
}
