/// An error related to market filters `PriceFilter`
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum PriceFilterError {
    #[error("Some price in MarketUpdate is too low.")]
    PriceTooLow,

    #[error("Some price in MarketUpdate is too high.")]
    PriceTooHigh,

    #[error("The price ({price}) in MarketUpdate does not conform to the step size {step_size}")]
    PriceStepSize { price: String, step_size: String },

    #[error("The bid ask spread does not exist in this MarketUpdate.")]
    InvalidBidAskSpread,

    #[error("The limit order price is lower than the low price multiple.")]
    LimitPriceBelowMultiple,

    #[error("The limit order price exceeds the maximum price multiple.")]
    LimitPriceAboveMultiple,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_price_filter_error() {
        assert_eq!(size_of::<PriceFilterError>(), 48);
    }
}
