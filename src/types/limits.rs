use getset::CopyGetters;

use super::{ConfigError, Error};

/// Defines the maximum order message rates, e.g.: limits order submission to 10 per second.
#[derive(Debug, Clone, CopyGetters)]
pub struct OrderRateLimits {
    /// How many orders can be submitted per second.
    #[getset(get_copy = "pub")]
    orders_per_second: u16,
}

impl Default for OrderRateLimits {
    fn default() -> Self {
        Self {
            orders_per_second: 10,
        }
    }
}

impl OrderRateLimits {
    /// Create a new instance if `orders_per_second` != 0
    pub fn new(orders_per_second: u16) -> crate::Result<Self> {
        if orders_per_second == 0 {
            return Err(Error::ConfigError(ConfigError::InvalidOrderLimits));
        }
        Ok(Self { orders_per_second })
    }
}
