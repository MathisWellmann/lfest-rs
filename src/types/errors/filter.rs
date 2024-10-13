use crate::prelude::{Mon, QuoteCurrency};

/// An error related to market filters `PriceFilter` and `QuantityFilter`.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum FilterError<I, const DB: u8, const DQ: u8>
where
    I: Mon<DB> + Mon<DQ>,
{
    #[error("Some price in MarketUpdate is too low.")]
    MarketUpdatePriceTooLow,

    #[error("Some price in MarketUpdate is too high.")]
    MarketUpdatePriceTooHigh,

    #[error("Some price in MarketUpdate does not conform to the step size")]
    MarketUpdatePriceStepSize {
        price: QuoteCurrency<I, DB, DQ>,
        step_size: QuoteCurrency<I, DB, DQ>,
    },

    #[error("The bid ask spread does not exist in this MarketUpdate.")]
    InvalidMarketUpdateBidAskSpread,
}
