//! This module contains order filtering related code

use getset::CopyGetters;
use num_traits::{One, Zero};

use crate::prelude::{ConfigError, CurrencyMarker, Mon, Monies, OrderError};

/// The `SizeFilter` defines the quantity rules that each order needs to follow
/// The generic currency `S` is always the `PairedCurrency` of the margin
/// currency
#[derive(Debug, Clone, CopyGetters)]
pub struct QuantityFilter<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    /// Defines the optional minimum `quantity` of any order
    #[getset(get_copy = "pub")]
    min_quantity: Option<Monies<T, BaseOrQuote>>,

    /// Defines the optional maximum `quantity` of any order
    #[getset(get_copy = "pub")]
    max_quantity: Option<Monies<T, BaseOrQuote>>,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % tick_size == 0
    #[getset(get_copy = "pub")]
    tick_size: Monies<T, BaseOrQuote>,
}

impl<T, BaseOrQuote> Default for QuantityFilter<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn default() -> Self {
        Self {
            min_quantity: None,
            max_quantity: None,
            tick_size: Monies::one(),
        }
    }
}

impl<T, BaseOrQuote> QuantityFilter<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    /// Create a new instance of the QuantityFilter.
    /// Make sure the `min_quantity` is a multiple of `tick_size`.
    pub fn new(
        min_quantity: Option<Monies<T, BaseOrQuote>>,
        max_quantity: Option<Monies<T, BaseOrQuote>>,
        tick_size: Monies<T, BaseOrQuote>,
    ) -> Result<Self, ConfigError> {
        if let Some(min_qty) = min_quantity {
            if (min_qty % tick_size) != Monies::zero() {
                return Err(ConfigError::InvalidMinQuantity);
            }
        }
        if tick_size == Monies::zero() {
            return Err(ConfigError::InvalidTickSize);
        }

        Ok(Self {
            min_quantity,
            max_quantity,
            tick_size,
        })
    }

    pub(crate) fn validate_order_quantity(
        &self,
        quantity: Monies<T, BaseOrQuote>,
    ) -> std::result::Result<(), OrderError<T>> {
        if quantity == Monies::zero() {
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
            Monies::zero()
        };

        if ((quantity - min_qty) % self.tick_size) != Monies::zero() {
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
