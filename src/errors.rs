/// Defines the possible order errors that can occur when submitting a new order
#[derive(thiserror::Error, Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum OrderError {
    #[error("Maximum number of active orders reached")]
    MaxActiveOrders,

    #[error("Invalid limit price of order")]
    InvalidLimitPrice,

    #[error("Invalid trigger price for order. e.g.: sell stop market order trigger price > ask")]
    InvalidTriggerPrice,

    #[error("order size must be > 0")]
    OrderSizeMustBePositive,

    #[error("The account does not have enough available balance to submit the order")]
    NotEnoughAvailableBalance,
}

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug, Clone)]
#[allow(missing_docs)]
pub enum Error {
    #[error("Wrong leverage provided")]
    ConfigWrongLeverage,

    #[error("Wrong starting balance provided")]
    ConfigWrongStartingBalance,

    #[error("could not parse")]
    ParseError,

    #[error("user order id not found")]
    UserOrderIdNotFound,

    #[error("internal order id not found")]
    OrderIdNotFound,

    #[error("Invalid position margin")]
    InvalidPositionMargin,

    #[error("Invalid order margin")]
    InvalidOrderMargin,

    #[error("Invalid available balance")]
    InvalidAvailableBalance,
}

/// This is defined as a convenience.
pub type Result<T> = std::result::Result<T, Error>;
