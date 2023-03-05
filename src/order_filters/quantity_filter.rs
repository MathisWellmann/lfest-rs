//! This module contains order filtering related code

use crate::{
    prelude::OrderError,
    types::{Currency, Order},
};

/// The `SizeFilter` defines the quantity rules that each order needs to follow
/// The generic currency `S` is always the `PairedCurrency` of the margin
/// currency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityFilter<S>
where S: Currency
{
    /// Defines the minimum `quantity` of any order
    /// Disabled if 0
    pub min_qty: S,

    /// Defines the maximum `quantity` of any order
    /// Disabled if 0
    pub max_qty: S,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % step_size == 0
    pub step_size: S,
}

impl<S> QuantityFilter<S>
where S: Currency
{
    pub(crate) fn validate_order(&self, order: &Order<S>) -> Result<(), OrderError> {
        if order.quantity() < self.min_qty {
            return Err(OrderError::QuantityTooLow);
        }
        if order.quantity() > self.max_qty {
            return Err(OrderError::QuantityTooHigh);
        }
        if ((order.quantity() - self.min_qty) % self.step_size) != S::new_zero() {
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
            min_qty: quote!(10),
            max_qty: quote!(1000),
            step_size: quote!(1),
        };

        let order = Order::market(Side::Buy, quote!(50)).unwrap();
        filter.validate_order(&order).unwrap();

        let order = Order::market(Side::Buy, quote!(5)).unwrap();
        assert_eq!(filter.validate_order(&order), Err(OrderError::QuantityTooLow));

        let order = Order::market(Side::Buy, quote!(5000)).unwrap();
        assert_eq!(filter.validate_order(&order), Err(OrderError::QuantityTooHigh));

        let order = Order::market(Side::Buy, quote!(50.5)).unwrap();
        assert_eq!(filter.validate_order(&order), Err(OrderError::InvalidQuantityStepSize));
    }
}
