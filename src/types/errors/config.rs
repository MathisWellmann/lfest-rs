#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

/// An error with the configuration.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(missing_docs, reason = "Self documenting")]
pub enum ConfigError {
    #[error("The specified leverage must be > 0")]
    InvalidLeverage,

    #[error("The provided starting balance must be > 0")]
    InvalidStartingBalance,

    #[error(
        "The chosen `tick_size` of the quantity filter does not work with the chosen `min_quantity`. `min_quantity` must be a multiple of `step_size`"
    )]
    InvalidMinQuantity,

    #[error("The chosen `min_price` must work with the chosen `tick_size`")]
    InvalidMinPrice,

    #[error("The chosen `tick` size is invalid.")]
    InvalidTickSize,

    #[error("The chosen `multiplier_up` must be greater than 1")]
    InvalidUpMultiplier,

    #[error("The chosen `multiplier_down` must be smaller than 1")]
    InvalidDownMultiplier,

    #[error("The maintenance margin fraction is invalid")]
    InvalidMaintenanceMarginFraction,

    #[error("Invalid order limits")]
    InvalidOrderLimits,

    #[error("The notional filter bounds are invalid")]
    InvalidNotionalFilter,

    #[error("The liquidation fee rate must be non-negative and less than 1")]
    InvalidLiquidationFee,

    #[error("The taker fee rate must be non-negative")]
    InvalidTakerFee,

    #[error("The maintenance margin tier configuration is invalid")]
    InvalidMaintenanceMarginTier,

    #[error("The stop protection collar must be strictly positive")]
    InvalidStopProtection,

    #[error("The slippage / market-impact model parameters are invalid")]
    InvalidSlippageModel,

    #[error("Two markets of a portfolio use the same ticker")]
    DuplicateMarketTicker,

    #[error("A portfolio supports at most 65536 markets (one per `MarketId`)")]
    TooManyMarkets,
}
