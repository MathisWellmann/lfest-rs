use crate::prelude::QuoteCurrency;

/// An error related to market filters `PriceFilter` and `QuantityFilter`.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum FilterError {
    #[error("Some price in MarketUpdate is too low.")]
    MarketUpdatePriceTooLow,

    #[error("Some price in MarketUpdate is too high.")]
    MarketUpdatePriceTooHigh,

    #[error("Some price in MarketUpdate does not conform to the step size")]
    MarketUpdatePriceStepSize {
        price: QuoteCurrency,
        step_size: QuoteCurrency,
    },

    #[error("The bid ask spread does not exist in this MarketUpdate.")]
    InvalidMarketUpdateBidAskSpread,
}
