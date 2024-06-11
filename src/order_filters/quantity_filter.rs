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
    pub min_quantity: Option<S>,

    /// Defines the maximum `quantity` of any order
    pub max_quantity: Option<S>,

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
            min_quantity: None,
            max_quantity: None,
            step_size: S::new(Dec!(1)),
        }
    }
}

impl<Q> QuantityFilter<Q>
where
    Q: Currency,
{
    pub(crate) fn validate_order_quantity(&self, quantity: Q) -> Result<(), OrderError> {
        if quantity == Q::new_zero() {
            return Err(OrderError::QuantityTooLow);
        }

        if let Some(max_qty) = self.max_quantity {
            if quantity > max_qty {
                return Err(OrderError::QuantityTooHigh);
            }
        }

        let min_qty = if let Some(min_qty) = self.min_quantity {
            if quantity < min_qty {
                return Err(OrderError::QuantityTooLow);
            }
            min_qty
        } else {
            Q::new_zero()
        };

        if ((quantity - min_qty) % self.step_size) != Q::new_zero() {
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
            min_quantity: Some(quote!(10)),
            max_quantity: Some(quote!(1000)),
            step_size: quote!(1),
        };

        assert_eq!(
            filter.validate_order_quantity(quote!(0)),
            Err(OrderError::QuantityTooLow)
        );

        filter.validate_order_quantity(quote!(50)).unwrap();

        assert_eq!(
            filter.validate_order_quantity(quote!(5)),
            Err(OrderError::QuantityTooLow)
        );

        assert_eq!(
            filter.validate_order_quantity(quote!(5000)),
            Err(OrderError::QuantityTooHigh)
        );

        assert_eq!(
            filter.validate_order_quantity(quote!(50.5)),
            Err(OrderError::InvalidQuantityStepSize)
        );
    }

    #[test]
    fn quantity_filter_2() {
        let filter = QuantityFilter {
            min_quantity: None,
            max_quantity: None,
            step_size: quote!(1),
        };
        assert_eq!(
            filter.validate_order_quantity(quote!(0)),
            Err(OrderError::QuantityTooLow)
        );
        assert_eq!(
            filter.validate_order_quantity(quote!(0.5)),
            Err(OrderError::InvalidQuantityStepSize)
        );
    }
}
