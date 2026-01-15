use thiserror::Error;

use super::FilterError;
use crate::order_rate_limiter::RateLimitReached;

/// Defines the possible order errors that can occur when submitting a new order
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum OrderError {
    #[error("The limit order price is lower than the low price multiple.")]
    LimitPriceBelowMultiple,

    #[error("The limit price is less than or equal zero.")]
    LimitPriceLTEZero,

    #[error("The limit order price exceeds the maximum price multiple.")]
    LimitPriceAboveMultiple,

    #[error(
        "The limit order `RePricing` was `GoodTillCrossing` leading to its rejection as the limit_price {limit_price} locks or crosses the away market quotation price {away_market_quotation_price}"
    )]
    GoodTillCrossingRejectedOrder {
        limit_price: String,
        away_market_quotation_price: String,
    },

    #[error("order size is less than or equal zero.")]
    OrderQuantityLTEZero,

    #[error("The order quantity is too low")]
    QuantityTooLow,

    #[error("The order quantity is too high")]
    QuantityTooHigh,

    #[error("The order quantity does not conform to the step size")]
    InvalidQuantityStepSize,

    #[error(transparent)]
    Filter(FilterError),
}

/// The possible errors that can occur when submitting a limit order.
#[derive(Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum SubmitLimitOrderError {
    #[error(transparent)]
    MaxNumberOfActiveLimitOrders(#[from] MaxNumberOfActiveOrders),

    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),
}

/// zero-sized error variant
#[derive(Error, Debug, Clone, derive_more::Display, Eq, PartialEq)]
pub struct MaxNumberOfActiveOrders;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_error() {
        assert_eq!(size_of::<MaxNumberOfActiveOrders>(), 0);
        assert_eq!(size_of::<SubmitLimitOrderError>(), 1);
    }
}
