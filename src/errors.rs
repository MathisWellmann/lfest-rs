#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Defines the possible order errors that can occur when submitting a new order
pub enum OrderError {
    /// Maximum number of active orders reached
    MaxActiveOrders,
    /// Invalid limit price of order
    InvalidLimitPrice,
    /// Invalid trigger price for order. e.g.: sell stop market order trigger price > ask
    InvalidTriggerPrice,
    /// Invalid order size
    InvalidOrderSize,
    /// The account does not have enough available balance to submit the order
    NotEnoughAvailableBalance,
}

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Config::new was provided an invalid leverage value
    #[error("Wrong leverage provided")]
    ConfigWrongLeverage,

    /// Config::new was provided an invalid starting balance
    #[error("Wrong starting balance provided")]
    ConfigWrongStartingBalance,

    /// When data could not be parsed
    #[error("could not parse from string")]
    ParseError,
}

/// This is defined as a convenience.
pub type Result<T> = std::result::Result<T, Error>;
