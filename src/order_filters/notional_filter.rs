//! Order notional filter enforcing minimum and maximum order notional value.

use getset::CopyGetters;

use crate::{
    prelude::{
        ConfigError,
        MarginCurrency,
        Mon,
    },
    types::ValidateOrderNotionalError,
};

/// Enforces optional minimum and maximum notional bounds on orders (measured in
/// the contract's margin currency).
///
/// Both bounds default to `None` so existing configurations behave identically
/// unless opted in.
#[derive(Debug, Clone, Copy, Eq, PartialEq, CopyGetters)]
pub struct NotionalFilter<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Optional minimum notional value an order must have.
    #[getset(get_copy = "pub")]
    min_notional: Option<Margin>,

    /// Optional maximum notional value an order may have.
    #[getset(get_copy = "pub")]
    max_notional: Option<Margin>,

    _i: std::marker::PhantomData<I>,
}

impl<I, const D: u8, Margin> Default for NotionalFilter<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    fn default() -> Self {
        Self {
            min_notional: None,
            max_notional: None,
            _i: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, Margin> NotionalFilter<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Create a new `NotionalFilter`.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidNotionalFilter`] if `min_notional <= 0`,
    /// `max_notional <= 0`, or `min_notional > max_notional`.
    pub fn new(
        min_notional: Option<Margin>,
        max_notional: Option<Margin>,
    ) -> Result<Self, ConfigError> {
        if let Some(min_n) = min_notional
            && min_n <= Margin::zero()
        {
            return Err(ConfigError::InvalidNotionalFilter);
        }
        if let Some(max_n) = max_notional
            && max_n <= Margin::zero()
        {
            return Err(ConfigError::InvalidNotionalFilter);
        }
        if let (Some(min_n), Some(max_n)) = (min_notional, max_notional)
            && min_n > max_n
        {
            return Err(ConfigError::InvalidNotionalFilter);
        }

        Ok(Self {
            min_notional,
            max_notional,
            _i: std::marker::PhantomData,
        })
    }

    /// Validate an order's notional value against the configured bounds.
    ///
    /// # Errors
    /// Returns [`ValidateOrderNotionalError::NotionalTooLow`] or
    /// [`ValidateOrderNotionalError::NotionalTooHigh`] when out of bounds.
    #[inline]
    pub fn validate_order_notional(
        &self,
        notional: Margin,
    ) -> Result<(), ValidateOrderNotionalError> {
        assert2::debug_assert!(notional >= Margin::zero());
        if let Some(min_n) = self.min_notional
            && notional < min_n
        {
            return Err(ValidateOrderNotionalError::NotionalTooLow);
        }
        if let Some(max_n) = self.max_notional
            && notional > max_n
        {
            return Err(ValidateOrderNotionalError::NotionalTooHigh);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::QuoteCurrency;

    #[test]
    fn notional_filter_validation_and_bounds() {
        assert_eq!(
            NotionalFilter::<i64, 5, QuoteCurrency<i64, 5>>::new(
                Some(QuoteCurrency::new(0, 0)),
                None
            ),
            Err(ConfigError::InvalidNotionalFilter)
        );
        assert_eq!(
            NotionalFilter::<i64, 5, QuoteCurrency<i64, 5>>::new(
                None,
                Some(QuoteCurrency::new(-1, 0))
            ),
            Err(ConfigError::InvalidNotionalFilter)
        );
        assert_eq!(
            NotionalFilter::<i64, 5, QuoteCurrency<i64, 5>>::new(
                Some(QuoteCurrency::new(100, 0)),
                Some(QuoteCurrency::new(50, 0))
            ),
            Err(ConfigError::InvalidNotionalFilter)
        );

        let filter = NotionalFilter::<i64, 5, QuoteCurrency<i64, 5>>::new(
            Some(QuoteCurrency::new(10, 0)),
            Some(QuoteCurrency::new(1000, 0)),
        )
        .unwrap();

        assert_eq!(
            filter.validate_order_notional(QuoteCurrency::new(9, 0)),
            Err(ValidateOrderNotionalError::NotionalTooLow)
        );
        assert_eq!(
            filter.validate_order_notional(QuoteCurrency::new(10, 0)),
            Ok(())
        );
        assert_eq!(
            filter.validate_order_notional(QuoteCurrency::new(1000, 0)),
            Ok(())
        );
        assert_eq!(
            filter.validate_order_notional(QuoteCurrency::new(1001, 0)),
            Err(ValidateOrderNotionalError::NotionalTooHigh)
        );
    }
}
