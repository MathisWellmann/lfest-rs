use super::{ConfigError, FilterError, OrderError, RiskError};
use crate::prelude::OrderId;

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
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

    #[error("The maximum number of active orders is reached")]
    MaxNumberOfActiveOrders,

    #[error("Could not convert the in")]
    IntegerConversion,

    #[error("Unable to create `Decimal`")]
    UnableToCreateDecimal,

    #[error("The order rate limit was reached for this period.")]
    RateLimitReached,

    #[error("The provided prices for `Candle` don't make sense.")]
    InvalidCandlePrices,
}
