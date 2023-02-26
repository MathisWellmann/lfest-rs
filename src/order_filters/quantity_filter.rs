//! This module contains order filtering related code

use crate::Currency;

/// The `SizeFilter` defines the quantity rules that each order needs to follow
/// The generic currency `S` is always the `PairedCurrency` of the margin
/// currency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityFilter<S>
where S: Currency
{
    /// Defines the minimum `quantity` of any order
    pub min_qty: S,

    /// Defines the maximum `quantity` of any order
    pub max_qty: S,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % step_size == 0
    pub step_size: S,
}
