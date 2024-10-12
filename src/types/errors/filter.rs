use crate::prelude::{Mon, Monies, Quote};

/// An error related to market filters `PriceFilter` and `QuantityFilter`.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum FilterError<T: Mon> {
    #[error("Some price in MarketUpdate is too low.")]
    MarketUpdatePriceTooLow,

    #[error("Some price in MarketUpdate is too high.")]
    MarketUpdatePriceTooHigh,

    #[error("Some price in MarketUpdate does not conform to the step size")]
    MarketUpdatePriceStepSize {
        price: Monies<T, Quote>,
        step_size: Monies<T, Quote>,
    },

    #[error("The bid ask spread does not exist in this MarketUpdate.")]
    InvalidMarketUpdateBidAskSpread,
}
