use super::{
    ConfigError,
    FilterError,
    OrderError,
    RiskError,
};
use crate::{
    order_rate_limiter::RateLimitReached,
    prelude::OrderId,
    types::MaxNumberOfActiveOrders,
};

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum Error {
    #[error(transparent)]
    FilterError(#[from] FilterError),

    #[error(transparent)]
    ConfigError(#[from] ConfigError),

    #[error(transparent)]
    OrderError(#[from] OrderError),

    #[error(transparent)]
    RiskError(#[from] RiskError),

    #[error("user order id not found")]
    UserOrderIdNotFound,

    #[error("internal order id not found")]
    OrderIdNotFound {
        /// The `OrderId` that was not found.
        order_id: OrderId,
    },

    #[error("The order is no longer active")]
    OrderNoLongerActive,

    #[error("Failed to lookup account.")]
    AccountLookupFailure,

    #[error(
        "The amended order quantity has already been filled in the original order. Remaining order was cancelled."
    )]
    AmendQtyAlreadyFilled,

    #[error("The constant decimal precision is incompatible")]
    WrongDecimalPrecision,

    #[error("Could not convert the in")]
    IntegerConversion,

    #[error("Unable to create `Decimal`")]
    UnableToCreateDecimal,

    #[error("The provided prices for `Candle` don't make sense.")]
    InvalidCandlePrices,

    #[error(transparent)]
    MaxNumberOfActiveLimitOrders(#[from] MaxNumberOfActiveOrders),

    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_error() {
        assert_eq!(size_of::<Error>(), 56);
    }
}
