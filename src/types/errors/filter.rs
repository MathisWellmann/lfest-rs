/// An error related to market filters `PriceFilter` and `QuantityFilter`.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum FilterError {
    #[error("Some price in MarketUpdate is too low.")]
    PriceTooLow,

    #[error("Some price in MarketUpdate is too high.")]
    PriceTooHigh,

    #[error("The price ({price}) in MarketUpdate does not conform to the step size {step_size}")]
    PriceStepSize { price: String, step_size: String },

    #[error("The bid ask spread does not exist in this MarketUpdate.")]
    InvalidBidAskSpread,
}
