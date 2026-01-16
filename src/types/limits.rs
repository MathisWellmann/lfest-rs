use std::num::NonZeroU32;

use getset::CopyGetters;
use serde::{
    Deserialize,
    Serialize,
};

use crate::EXPECT_NON_ZERO;

/// Defines the maximum order message rates, e.g.: limits order submission to 10 per second.
#[derive(Debug, Clone, Copy, CopyGetters, Serialize, Deserialize)]
pub struct OrderRateLimits {
    /// How many orders can be submitted per second.
    #[getset(get_copy = "pub")]
    orders_per_second: NonZeroU32,
}

impl Default for OrderRateLimits {
    fn default() -> Self {
        Self {
            orders_per_second: NonZeroU32::new(10).expect(EXPECT_NON_ZERO),
        }
    }
}

impl OrderRateLimits {
    /// Create a new instance if `orders_per_second` != 0
    #[inline]
    pub fn new(orders_per_second: NonZeroU32) -> Self {
        Self { orders_per_second }
    }
}
