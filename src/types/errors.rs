use crate::risk_engine::RiskError;

/// Defines the possible order errors that can occur when submitting a new order
#[derive(thiserror::Error, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum OrderError {
    #[error("Maximum number of active orders reached")]
    MaxActiveOrders,

    #[error("The limit order price is lower than the low price multiple.")]
    LimitPriceBelowMultiple,

    #[error("The limit order price is lower than the minimum price filter.")]
    LimitPriceBelowMin,

    #[error("The limit price is below zero")]
    LimitPriceBelowZero,

    #[error("The limit order price exceeds the maximum price multiple.")]
    LimitPriceAboveMultiple,

    #[error("The limit price is above the maximum price.")]
    LimitPriceAboveMax,

    #[error("The limit price is larger than the current ask")]
    LimitPriceAboveAsk,

    #[error("The limit price is lower than the current bid")]
    LimitPriceBelowBid,

    #[error("The order price does not conform to the step size.")]
    InvalidOrderPriceStepSize,

    #[error("Invalid trigger price for order. e.g.: sell stop market order trigger price > ask")]
    InvalidTriggerPrice,

    #[error("order size must be > 0")]
    OrderSizeMustBePositive,

    #[error("The account does not have enough available balance to submit the order")]
    NotEnoughAvailableBalance,

    #[error("The order quantity is too low")]
    QuantityTooLow,

    #[error("The order quantity is too high")]
    QuantityTooHigh,

    #[error("The order quantity does not conform to the step size")]
    InvalidQuantityStepSize,
}

/// Describes possible Errors that may occur when calling methods in this crate
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
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

    #[error("The max_num_open_orders must be > 0")]
    InvalidMaxNumOpenOrders,

    #[error("The provided starting balance must be > 0")]
    InvalidStartingBalance,

    #[error("Some price in MarketUpdate is too low.")]
    MarketUpdatePriceTooLow,

    #[error("Some price in MarketUpdate is too high.")]
    MarketUpdatePriceTooHigh,

    #[error("Some price in MarketUpdate does not conform to the step size")]
    MarketUpdatePriceStepSize,

    #[error("The bid ask spread does not exist in this MarketUpdate.")]
    InvalidMarketUpdateBidAskSpread,

    #[error("An invalid price was provided in MarketUpdate")]
    InvalidMarketUpdatePrice,

    #[error("The Account does not have enough available balance.")]
    NotEnoughAvailableBalance,

    #[error("Not enough order margin available")]
    NotEnoughOrderMargin,

    #[error("Not enough position margin available")]
    NotEnoughPositionMargin,

    #[error("The provided amount is invalid.")]
    InvalidAmount,

    #[error("The provided price is invalid.")]
    InvalidPrice,

    #[error("The long position needs to be closed first.")]
    OpenLong,

    #[error("The short position needs to be closed first.")]
    OpenShort,

    #[error("The provided value is not positive")]
    NonPositive,

    #[error(transparent)]
    OrderError(#[from] OrderError),

    #[error(transparent)]
    RiskError(#[from] RiskError),

    #[error("The specified leverage must be > 0")]
    InvalidLeverage,

    #[error(transparent)]
    Decimal(#[from] fpdec::DecimalError),
}

/// This is defined as a convenience.
pub type Result<T> = std::result::Result<T, Error>;
