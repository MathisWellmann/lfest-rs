//! This module contains order filtering related code

use fpdec::{Dec, Decimal};
use getset::CopyGetters;

use crate::{
    prelude::{Error, OrderError},
    types::Currency,
    Result,
};

/// The `SizeFilter` defines the quantity rules that each order needs to follow
/// The generic currency `S` is always the `PairedCurrency` of the margin
/// currency
#[derive(Debug, Clone, CopyGetters)]
pub struct QuantityFilter<S>
where
    S: Currency,
{
    /// Defines the optional minimum `quantity` of any order
    #[getset(get_copy = "pub")]
    min_quantity: Option<S>,

    /// Defines the optional maximum `quantity` of any order
    #[getset(get_copy = "pub")]
    max_quantity: Option<S>,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % tick_size == 0
    #[getset(get_copy = "pub")]
    tick_size: S,
}

impl<S> Default for QuantityFilter<S>
where
    S: Currency,
{
    fn default() -> Self {
        Self {
            min_quantity: None,
            max_quantity: None,
            tick_size: S::new(Dec!(1)),
        }
    }
}

impl<Q> QuantityFilter<Q>
where
    Q: Currency,
{
    /// Create a new instance of the QuantityFilter.
    /// Make sure the `min_quantity` is a multiple of `tick_size`.
    pub fn new(min_quantity: Option<Q>, max_quantity: Option<Q>, tick_size: Q) -> Result<Self> {
        if let Some(min_qty) = min_quantity {
            if (min_qty % tick_size) != Q::new_zero() {
                return Err(Error::InvalidMinQuantity);
            }
        }
        if tick_size == Q::new_zero() {
            return Err(Error::InvalidTickSize);
        }

        Ok(Self {
            min_quantity,
            max_quantity,
            tick_size,
        })
    }

    pub(crate) fn validate_order_quantity(
        &self,
        quantity: Q,
    ) -> std::result::Result<(), OrderError> {
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

        if ((quantity - min_qty) % self.tick_size) != Q::new_zero() {
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
            tick_size: quote!(1),
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
            tick_size: quote!(1),
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
