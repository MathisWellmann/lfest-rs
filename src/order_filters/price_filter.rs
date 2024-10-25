use const_decimal::Decimal;
use getset::CopyGetters;
use num_traits::{One, Zero};

use crate::prelude::{ConfigError, FilterError, Mon, OrderError, QuoteCurrency};

/// The `PriceFilter` defines the price rules for a symbol
#[derive(Debug, Clone, CopyGetters)]
pub struct PriceFilter<I, const D: u8>
where
    I: Mon<D>,
{
    /// Defines the optional minimum price allowed.
    #[getset(get_copy = "pub")]
    min_price: Option<QuoteCurrency<I, D>>,

    /// Defines the optional maximum price allowed.
    #[getset(get_copy = "pub")]
    max_price: Option<QuoteCurrency<I, D>>,

    /// Defines the intervals that a price can be increased / decreased by.
    /// For the filter to pass,
    /// (order.limit_price - min_price) % tick_size == 0
    #[getset(get_copy = "pub")]
    tick_size: QuoteCurrency<I, D>,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price <= mark_price * multiplier_up
    #[getset(get_copy = "pub")]
    multiplier_up: Decimal<I, D>,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price >= mark_price * multiplier_down
    #[getset(get_copy = "pub")]
    multiplier_down: Decimal<I, D>,
}

impl<I, const D: u8> Default for PriceFilter<I, D>
where
    I: Mon<D>,
{
    fn default() -> Self {
        Self {
            min_price: None,
            max_price: None,
            tick_size: QuoteCurrency::from(Decimal::one()),
            multiplier_up: Decimal::TWO,
            multiplier_down: Decimal::zero(),
        }
    }
}

impl<I, const D: u8> PriceFilter<I, D>
where
    I: Mon<D>,
{
    /// Create a new `PriceFilter`.
    pub fn new(
        min_price: Option<QuoteCurrency<I, D>>,
        max_price: Option<QuoteCurrency<I, D>>,
        tick_size: QuoteCurrency<I, D>,
        multiplier_up: Decimal<I, D>,
        multiplier_down: Decimal<I, D>,
    ) -> Result<Self, ConfigError> {
        if let Some(min_qty) = min_price {
            if (min_qty % tick_size) != QuoteCurrency::zero() {
                return Err(ConfigError::InvalidMinPrice);
            }
        }

        if tick_size.is_zero() {
            return Err(ConfigError::InvalidTickSize);
        }

        if multiplier_up <= Decimal::one() {
            return Err(ConfigError::InvalidUpMultiplier);
        }

        if multiplier_down >= Decimal::one() {
            return Err(ConfigError::InvalidDownMultiplier);
        }

        Ok(Self {
            min_price,
            max_price,
            tick_size,
            multiplier_up,
            multiplier_down,
        })
    }

    /// check if an `Order` is valid
    pub fn validate_limit_price(
        &self,
        limit_price: QuoteCurrency<I, D>,
        mark_price: QuoteCurrency<I, D>,
    ) -> Result<(), OrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(OrderError::LimitPriceBelowMin);
        }

        if let Some(max_price) = self.max_price {
            if limit_price > max_price {
                return Err(OrderError::LimitPriceAboveMax);
            }
        }

        let min_price = if let Some(min_price) = self.min_price {
            if limit_price < min_price {
                return Err(OrderError::LimitPriceBelowMin);
            }
            min_price
        } else {
            QuoteCurrency::zero()
        };

        if ((limit_price - min_price) % self.tick_size) != QuoteCurrency::zero() {
            return Err(OrderError::InvalidOrderPriceStepSize);
        }
        if limit_price > mark_price * self.multiplier_up && self.multiplier_up != Decimal::zero() {
            return Err(OrderError::LimitPriceAboveMultiple);
        }
        if limit_price < mark_price * self.multiplier_down
            && self.multiplier_down != Decimal::zero()
        {
            return Err(OrderError::LimitPriceBelowMultiple);
        }
        Ok(())
    }
}

/// Errors if there is no bid-ask spread
pub(crate) fn enforce_bid_ask_spread<I, const D: u8>(
    bid: QuoteCurrency<I, D>,
    ask: QuoteCurrency<I, D>,
) -> Result<(), FilterError>
where
    I: Mon<D>,
{
    if bid >= ask {
        return Err(FilterError::InvalidMarketUpdateBidAskSpread);
    }
    Ok(())
}

/// Make sure the price is not too low
/// Disabled if `min_price` == 0
pub(crate) fn enforce_min_price<I, const D: u8>(
    min_price: Option<QuoteCurrency<I, D>>,
    price: QuoteCurrency<I, D>,
) -> Result<(), FilterError>
where
    I: Mon<D>,
{
    if let Some(min_price) = min_price {
        if price < min_price && min_price != QuoteCurrency::zero() {
            return Err(FilterError::MarketUpdatePriceTooLow);
        }
    }
    Ok(())
}

/// Make sure the price is not too high
/// Disabled if `max_price` == 0
pub(crate) fn enforce_max_price<I, const D: u8>(
    max_price: Option<QuoteCurrency<I, D>>,
    price: QuoteCurrency<I, D>,
) -> Result<(), FilterError>
where
    I: Mon<D>,
{
    if let Some(max_price) = max_price {
        if price > max_price && max_price != QuoteCurrency::zero() {
            return Err(FilterError::MarketUpdatePriceTooHigh);
        }
    }
    Ok(())
}

/// Make sure the price conforms to the step size
pub(crate) fn enforce_step_size<I, const D: u8>(
    step_size: QuoteCurrency<I, D>,
    price: QuoteCurrency<I, D>,
) -> Result<(), FilterError>
where
    I: Mon<D>,
{
    if (price % step_size) != QuoteCurrency::zero() {
        return Err(FilterError::MarketUpdatePriceStepSize {
            price: price.to_string(),
            step_size: step_size.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn price_filter() {
        let filter = PriceFilter {
            min_price: Some(QuoteCurrency::<i32, 2>::new(1, 1)),
            max_price: Some(QuoteCurrency::new(1000, 0)),
            tick_size: QuoteCurrency::new(1, 1),
            multiplier_up: Decimal::try_from_scaled(12, 1).unwrap(),
            multiplier_down: Decimal::try_from_scaled(8, 1).unwrap(),
        };
        let mark_price = QuoteCurrency::new(100, 0);

        // Some passing orders
        let price = QuoteCurrency::new(99, 0);
        filter.validate_limit_price(price, mark_price).unwrap();
        let price = QuoteCurrency::new(99, 0);
        filter.validate_limit_price(price, mark_price).unwrap();

        // beyond max and min
        let price = QuoteCurrency::new(5, 2);
        assert_eq!(
            filter.validate_limit_price(price, mark_price),
            Err(OrderError::LimitPriceBelowMin)
        );
        let price = QuoteCurrency::new(1001, 0);
        assert_eq!(
            filter.validate_limit_price(price, mark_price),
            Err(OrderError::LimitPriceAboveMax)
        );

        // Test upper price band
        let price = QuoteCurrency::new(120, 0);
        filter.validate_limit_price(price, mark_price).unwrap();
        let price = QuoteCurrency::new(121, 0);
        assert_eq!(
            filter.validate_limit_price(price, mark_price),
            Err(OrderError::LimitPriceAboveMultiple)
        );

        // Test lower price band
        let price = QuoteCurrency::new(80, 0);
        filter.validate_limit_price(price, mark_price).unwrap();
        let price = QuoteCurrency::new(79, 0);
        assert_eq!(
            filter.validate_limit_price(price, mark_price),
            Err(OrderError::LimitPriceBelowMultiple)
        );

        // Test step size
        let price = QuoteCurrency::new(10005, 2);
        assert_eq!(
            filter.validate_limit_price(price, mark_price),
            Err(OrderError::InvalidOrderPriceStepSize)
        );
    }
}
