//! This module contains order filtering related code

use fpdec::{Dec, Decimal};

use crate::{prelude::OrderError, types::Currency};

/// The `SizeFilter` defines the quantity rules that each order needs to follow
/// The generic currency `S` is always the `PairedCurrency` of the margin
/// currency
#[derive(Debug, Clone)]
pub struct QuantityFilter<S>
where
    S: Currency,
{
    /// Defines the minimum `quantity` of any order
    /// Disabled if 0
    pub min_quantity: S,

    /// Defines the maximum `quantity` of any order
    /// Disabled if 0
    pub max_quantity: S,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % step_size == 0
    pub step_size: S,
}

impl<S> Default for QuantityFilter<S>
where
    S: Currency,
{
    fn default() -> Self {
        Self {
            min_quantity: S::new_zero(),
            max_quantity: S::new_zero(),
            step_size: S::new(Dec!(1)),
        }
    }
}

impl<Q> QuantityFilter<Q>
where
    Q: Currency,
{
    pub(crate) fn validate_order_quantity(&self, quantity: Q) -> Result<(), OrderError> {
        if quantity < self.min_quantity && self.min_quantity != Q::new_zero() {
            return Err(OrderError::QuantityTooLow);
        }
        if quantity > self.max_quantity && self.max_quantity != Q::new_zero() {
            return Err(OrderError::QuantityTooHigh);
        }
        if ((quantity - self.min_quantity) % self.step_size) != Q::new_zero() {
            return Err(OrderError::InvalidQuantityStepSize);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn quantity_filter() {
        let filter = QuantityFilter {
            min_quantity: quote!(10),
            max_quantity: quote!(1000),
            step_size: quote!(1),
        };

        let order = MarketOrder::new(Side::Buy, quote!(50)).unwrap();
        filter.validate_order_quantity(order.quantity()).unwrap();

        let order = MarketOrder::new(Side::Buy, quote!(5)).unwrap();
        assert_eq!(
            filter.validate_order_quantity(order.quantity()),
            Err(OrderError::QuantityTooLow)
        );

        let order = MarketOrder::new(Side::Buy, quote!(5000)).unwrap();
        assert_eq!(
            filter.validate_order_quantity(order.quantity()),
            Err(OrderError::QuantityTooHigh)
        );

        let order = MarketOrder::new(Side::Buy, quote!(50.5)).unwrap();
        assert_eq!(
            filter.validate_order_quantity(order.quantity()),
            Err(OrderError::InvalidQuantityStepSize)
        );
    }
}
