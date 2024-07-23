use super::{ConfigError, FilterError, OrderError, RiskError};

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
    OrderIdNotFound,

    #[error(transparent)]
    Decimal(#[from] fpdec::DecimalError),

    #[error("Failed to lookup account.")]
    AccountLookupFailure,

    #[error("The amended order quantity has already been filled in the original order. Remaining order was cancelled.")]
    AmendQtyAlreadyFilled,
}
