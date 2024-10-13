use super::{ConfigError, FilterError, OrderError, RiskError};
use crate::prelude::{Mon, OrderId};

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Error<I, const DB: u8, const DQ: u8>
where
    I: Mon<DB> + Mon<DQ>,
{
    #[error(transparent)]
    FilterError(#[from] FilterError<I, DB, DQ>),

    #[error(transparent)]
    ConfigError(#[from] ConfigError),

    #[error(transparent)]
    OrderError(#[from] OrderError<I, DB, DQ>),

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

    #[error("The amended order quantity has already been filled in the original order. Remaining order was cancelled.")]
    AmendQtyAlreadyFilled,

    #[error("The constant decimal precision is incompatible")]
    WrongDecimalPrecision,
}
