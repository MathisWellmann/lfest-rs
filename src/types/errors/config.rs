/// An error with the configuration.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum ConfigError {
    #[error("The specified leverage must be > 0")]
    InvalidLeverage,

    #[error("The provided starting balance must be > 0")]
    InvalidStartingBalance,

    #[error("The max_num_open_orders must be > 0")]
    InvalidMaxNumOpenOrders,

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
}
