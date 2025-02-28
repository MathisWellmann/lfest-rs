/// Defines the possible order errors that can occur when submitting a new order
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum OrderError {
    #[error("The limit order price is lower than the low price multiple.")]
    LimitPriceBelowMultiple,

    #[error("The limit order price is lower than the minimum price filter.")]
    LimitPriceBelowMin,

    #[error("The limit price is less than or equal zero.")]
    LimitPriceLTEZero,

    #[error("The limit order price exceeds the maximum price multiple.")]
    LimitPriceAboveMultiple,

    #[error("The limit price is above the maximum price.")]
    LimitPriceAboveMax,

    #[error("The limit order `RePricing` was `GoodTillCrossing` leading to its rejection as the limit_price {limit_price} locks or crosses the away market quotation price {away_market_quotation_price}")]
    GoodTillCrossingRejectedOrder {
        limit_price: String,
        away_market_quotation_price: String,
    },

    #[error("The order price does not conform to the step size.")]
    InvalidOrderPriceStepSize {
        limit_price: String,
        step_size: String,
    },

    #[error("order size is less than or equal zero.")]
    OrderQuantityLTEZero,

    #[error("The order quantity is too low")]
    QuantityTooLow,

    #[error("The order quantity is too high")]
    QuantityTooHigh,

    #[error("The order quantity does not conform to the step size")]
    InvalidQuantityStepSize,
}
