//! This module contains order filtering related code

use getset::CopyGetters;

use crate::{
    prelude::{
        ConfigError,
        Currency,
        Mon,
        QuoteCurrency,
    },
    types::ValidateOrderQuantityError,
};

/// The `SizeFilter` defines the quantity rules that each order needs to follow
#[derive(Debug, Clone, CopyGetters)]
pub struct QuantityFilter<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Defines the optional minimum `quantity` of any order
    #[getset(get_copy = "pub")]
    min_quantity: Option<BaseOrQuote>,

    /// Defines the optional maximum `quantity` of any order
    #[getset(get_copy = "pub")]
    max_quantity: Option<BaseOrQuote>,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % tick_size == 0
    #[getset(get_copy = "pub")]
    tick_size: BaseOrQuote,

    _quote: std::marker::PhantomData<QuoteCurrency<I, D>>,
}

impl<I, const D: u8, BaseOrQuote> Default for QuantityFilter<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn default() -> Self {
        Self {
            min_quantity: None,
            max_quantity: None,
            tick_size: BaseOrQuote::one(),
            _quote: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> QuantityFilter<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Create a new instance of the QuantityFilter.
    /// Make sure the `min_quantity` is a multiple of `tick_size`.
    pub fn new(
        min_quantity: Option<BaseOrQuote>,
        max_quantity: Option<BaseOrQuote>,
        tick_size: BaseOrQuote,
    ) -> Result<Self, ConfigError> {
        if let Some(min_qty) = min_quantity
            && (min_qty % tick_size) != BaseOrQuote::zero()
        {
            return Err(ConfigError::InvalidMinQuantity);
        }
        if tick_size == BaseOrQuote::zero() {
            return Err(ConfigError::InvalidTickSize);
        }

        Ok(Self {
            min_quantity,
            max_quantity,
            tick_size,
            _quote: std::marker::PhantomData,
        })
    }

    pub(crate) fn validate_order_quantity(
        &self,
        quantity: BaseOrQuote,
    ) -> Result<(), ValidateOrderQuantityError> {
        if quantity == BaseOrQuote::zero() {
            return Err(ValidateOrderQuantityError::QuantityTooLow);
        }

        if let Some(max_qty) = self.max_quantity
            && quantity > max_qty
        {
            return Err(ValidateOrderQuantityError::QuantityTooHigh);
        }

        let min_qty = if let Some(min_qty) = self.min_quantity {
            if quantity < min_qty {
                return Err(ValidateOrderQuantityError::QuantityTooLow);
            }
            min_qty
        } else {
            BaseOrQuote::zero()
        };

        if ((quantity - min_qty) % self.tick_size) != BaseOrQuote::zero() {
            return Err(ValidateOrderQuantityError::InvalidQuantityStepSize);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use num_traits::{
        One,
        Zero,
    };

    use super::*;
    use crate::prelude::*;

    #[test]
    fn quantity_filter() {
        let filter = QuantityFilter {
            min_quantity: Some(QuoteCurrency::<i32, 2>::new(10, 0)),
            max_quantity: Some(QuoteCurrency::new(1000, 0)),
            tick_size: QuoteCurrency::one(),
            _quote: std::marker::PhantomData,
        };

        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::zero()),
            Err(ValidateOrderQuantityError::QuantityTooLow)
        );

        filter
            .validate_order_quantity(QuoteCurrency::new(50, 0))
            .unwrap();

        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(5, 0)),
            Err(ValidateOrderQuantityError::QuantityTooLow)
        );

        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(5000, 0)),
            Err(ValidateOrderQuantityError::QuantityTooHigh)
        );

        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(505, 1)),
            Err(ValidateOrderQuantityError::InvalidQuantityStepSize)
        );
    }

    #[test]
    fn quantity_filter_2() {
        let filter = QuantityFilter {
            min_quantity: None,
            max_quantity: None,
            tick_size: QuoteCurrency::one(),
            _quote: std::marker::PhantomData::<QuoteCurrency<i32, 2>>,
        };
        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::zero()),
            Err(ValidateOrderQuantityError::QuantityTooLow)
        );
        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(5, 1)),
            Err(ValidateOrderQuantityError::InvalidQuantityStepSize)
        );
    }

    #[test]
    fn quantity_filter_boundaries_are_inclusive() {
        let filter = QuantityFilter {
            min_quantity: Some(QuoteCurrency::<i64, 2>::new(10, 0)),
            max_quantity: Some(QuoteCurrency::new(1_000, 0)),
            tick_size: QuoteCurrency::one(),
            _quote: std::marker::PhantomData,
        };

        // Exactly the minimum and maximum pass.
        filter
            .validate_order_quantity(QuoteCurrency::new(10, 0))
            .unwrap();
        filter
            .validate_order_quantity(QuoteCurrency::new(1_000, 0))
            .unwrap();

        // One tick outside either bound fails.
        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(9, 0)),
            Err(ValidateOrderQuantityError::QuantityTooLow)
        );
        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(1_001, 0)),
            Err(ValidateOrderQuantityError::QuantityTooHigh)
        );
    }

    #[test]
    fn quantity_filter_checks_the_step_relative_to_the_minimum() {
        let filter = QuantityFilter {
            min_quantity: Some(QuoteCurrency::<i64, 2>::new(10, 0)),
            max_quantity: None,
            tick_size: QuoteCurrency::new(3, 0),
            _quote: std::marker::PhantomData,
        };

        // (13 - 10) % 3 == 0, but (13 + 10) % 3 != 0.
        filter
            .validate_order_quantity(QuoteCurrency::new(13, 0))
            .unwrap();
        // (14 - 10) % 3 != 0.
        assert_eq!(
            filter.validate_order_quantity(QuoteCurrency::new(14, 0)),
            Err(ValidateOrderQuantityError::InvalidQuantityStepSize)
        );
    }

    #[test]
    fn quantity_filter_constructor_rejects_invalid_config() {
        // The minimum must lie on the tick grid.
        assert!(matches!(
            QuantityFilter::<i64, 2, QuoteCurrency<i64, 2>>::new(
                Some(QuoteCurrency::new(5, 0)),
                None,
                QuoteCurrency::new(2, 0),
            ),
            Err(ConfigError::InvalidMinQuantity)
        ));
        // A zero tick is invalid. `min_quantity` is `None` because the constructor
        // checks the modulus first, which would divide by zero.
        assert!(matches!(
            QuantityFilter::<i64, 2, QuoteCurrency<i64, 2>>::new(None, None, QuoteCurrency::zero(),),
            Err(ConfigError::InvalidTickSize)
        ));
        QuantityFilter::<i64, 2, QuoteCurrency<i64, 2>>::new(
            Some(QuoteCurrency::new(4, 0)),
            None,
            QuoteCurrency::new(2, 0),
        )
        .unwrap();
    }

    #[test]
    fn size_of_quantity_filter() {
        assert_eq!(
            size_of::<QuantityFilter<i64, 5, BaseCurrency<i64, 5>>>(),
            40
        );
    }
}
