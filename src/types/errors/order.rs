// TODO: Split this into more files.

use std::fmt::Display;

use thiserror::Error;

use crate::{
    order_rate_limiter::RateLimitReached,
    types::{
        NotEnoughAvailableBalance,
        OrderId,
    },
};

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

#[derive(Error, Debug, Clone, Eq, PartialEq, derive_more::Display)]
#[allow(missing_docs, reason = "Self documenting")]
pub struct MaxNumberOfActiveOrders(pub u16);

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
        assert_eq!(size_of::<MaxNumberOfActiveOrders>(), 2);

        assert_eq!(size_of::<OrderIdNotFound<()>>(), 16);
        assert_eq!(size_of::<OrderIdNotFound<u32>>(), 16);
        assert_eq!(size_of::<OrderIdNotFound<u64>>(), 16);
    }
}
