#[derive(thiserror::Error, Debug, Clone, Copy, Serialize, Deserialize)]
/// Defines the possible order errors that can occur when submitting a new order
pub enum OrderError {
    /// Maximum number of active orders reached
    #[error("Maximum number of active orders reached")]
    MaxActiveOrders,

    /// Invalid limit price of order
    #[error("Invalid limit price of order")]
    InvalidLimitPrice,

    /// Invalid trigger price for order
    #[error("Invalid trigger price for order. e.g.: sell stop market order trigger price > ask")]
    InvalidTriggerPrice,

    /// Invalid order size
    #[error("invalid order size")]
    InvalidOrderSize,

    /// The account does not have enought available balance to submit the order
    #[error("The account does not have enough available balance to submit the order")]
    NotEnoughAvailableBalance,
}

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug, Clone)]
#[allow(missing_docs)]
pub enum Error {
    /// Config::new was provided an invalid leverage value
    #[error("Wrong leverage provided")]
    ConfigWrongLeverage,

    /// Config::new was provided an invalid starting balance
    #[error("Wrong starting balance provided")]
    ConfigWrongStartingBalance,

    /// When data could not be parsed
    #[error("could not parse")]
    ParseError,

    /// when cancelling an order but no matching user order id is found
    #[error("user order id not found")]
    UserOrderIdNotFound,

    /// When the internal order id is not found while cancelling order
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
