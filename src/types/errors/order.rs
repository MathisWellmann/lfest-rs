use crate::prelude::QuoteCurrency;

/// Defines the possible order errors that can occur when submitting a new order
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
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

    #[error("The limit price {limit_price} is greater or equal the current best ask {best_ask}")]
    LimitPriceGteAsk {
        limit_price: QuoteCurrency,
        best_ask: QuoteCurrency,
    },

    #[error("The limit price {limit_price} is lower or equal the current best bid {best_bid}")]
    LimitPriceLteBid {
        limit_price: QuoteCurrency,
        best_bid: QuoteCurrency,
    },

    #[error("The order price does not conform to the step size.")]
    InvalidOrderPriceStepSize,

    #[error("order size is less than or equal zero.")]
    OrderQuantityLTEZero,

    #[error("The order quantity is too low")]
    QuantityTooLow,

    #[error("The order quantity is too high")]
    QuantityTooHigh,

    #[error("The order quantity does not conform to the step size")]
    InvalidQuantityStepSize,
}
