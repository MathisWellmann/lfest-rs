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

impl<I, const D: u8> std::fmt::Display for PriceFilter<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let min_price = match self.min_price {
            Some(p) => p.to_string(),
            None => "None".to_string(),
        };
        let max_price = match self.max_price {
            Some(p) => p.to_string(),
            None => "None".to_string(),
        };
        write!(
            f,
            "PriceFilter( min_price: {}, max_price: {}, tick_size: {}, multiplier_up: {}, multiplier_down: {} )",
            min_price, max_price, self.tick_size, self.multiplier_up, self.multiplier_down,
        )
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
        if let Some(min_qty) = min_price
            && (min_qty % tick_size) != QuoteCurrency::zero()
        {
            return Err(ConfigError::InvalidMinPrice);
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
        enforce_max_price(self.max_price, limit_price).map_err(OrderError::Filter)?;
        enforce_min_price(self.min_price, limit_price).map_err(OrderError::Filter)?;
        enforce_step_size(self.tick_size, limit_price).map_err(OrderError::Filter)?;

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
        return Err(FilterError::InvalidBidAskSpread);
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
    if price <= QuoteCurrency::zero() {
        return Err(FilterError::PriceTooLow);
    }
    if let Some(min_price) = min_price {
        assert2::debug_assert!(min_price != QuoteCurrency::zero());
        if price < min_price {
            return Err(FilterError::PriceTooLow);
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
        assert2::debug_assert!(max_price != QuoteCurrency::zero());
        if price > max_price {
            return Err(FilterError::PriceTooHigh);
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
        return Err(FilterError::PriceStepSize {
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

    #[test_case::test_matrix(
        [80, 90, 99, 100],
        [101, 105, 110]
    )]
    fn price_filter_enforce_bid_ask_spread(bid: i64, ask: i64) {
        enforce_bid_ask_spread(
            QuoteCurrency::<i64, 5>::new(bid, 0),
            QuoteCurrency::new(ask, 0),
        )
        .unwrap();
    }

    #[test_case::test_matrix(
        [101, 105, 110],
        [80, 90, 99, 100]
    )]
    fn price_filter_enforce_bid_ask_spread_err(bid: i64, ask: i64) {
        assert_eq!(
            enforce_bid_ask_spread(
                QuoteCurrency::<i64, 5>::new(bid, 0),
                QuoteCurrency::new(ask, 0),
            ),
            Err(FilterError::InvalidBidAskSpread)
        );
    }

    #[test_case::test_matrix([1, 2, 3, 5, 10])]
    fn price_filter_enforce_min_price_none(price: i64) {
        enforce_min_price(None, QuoteCurrency::<i64, 5>::new(price, 1)).unwrap();
    }

    #[test_case::test_matrix([5, 6, 7, 10])]
    fn price_filter_enforce_min_price(price: i64) {
        enforce_min_price(
            Some(QuoteCurrency::new(5, 1)),
            QuoteCurrency::<i64, 5>::new(price, 1),
        )
        .unwrap();
    }

    #[test_case::test_matrix([1, 2, 3, 4])]
    fn price_filter_enforce_min_price_err(price: i64) {
        assert_eq!(
            enforce_min_price(
                Some(QuoteCurrency::new(5, 1)),
                QuoteCurrency::<i64, 5>::new(price, 1),
            ),
            Err(FilterError::PriceTooLow)
        )
    }

    #[test_case::test_matrix([1, 2, 3, 5, 10, 100, 1000])]
    fn price_filter_enforce_max_price_none(price: i64) {
        enforce_max_price(None, QuoteCurrency::<i64, 5>::new(price, 0)).unwrap();
    }

    #[test_case::test_matrix([1, 2, 3, 5, 10, 100])]
    fn price_filter_enforce_max_price(price: i64) {
        enforce_max_price(
            Some(QuoteCurrency::new(100, 0)),
            QuoteCurrency::<i64, 5>::new(price, 0),
        )
        .unwrap();
    }

    #[test_case::test_matrix([101, 102, 1000, 10000])]
    fn price_filter_enforce_max_price_err(price: i64) {
        assert_eq!(
            enforce_max_price(
                Some(QuoteCurrency::new(100, 0)),
                QuoteCurrency::<i64, 5>::new(price, 0),
            ),
            Err(FilterError::PriceTooHigh)
        );
    }

    #[test_case::test_matrix([100, 105, 110, 115])]
    fn price_filter_enforce_step_size(price: i64) {
        enforce_step_size(
            QuoteCurrency::new(5, 1),
            QuoteCurrency::<i64, 5>::new(price, 1),
        )
        .unwrap();
    }

    #[test_case::test_matrix([101, 102, 103, 104, 106, 107, 108, 109])]
    fn price_filter_enforce_step_size_err(price: i64) {
        let step_size = QuoteCurrency::<i64, 5>::new(5, 1);
        let price = QuoteCurrency::new(price, 1);

        assert_eq!(
            enforce_step_size(step_size, price),
            Err(FilterError::PriceStepSize {
                price: price.to_string(),
                step_size: step_size.to_string(),
            })
        );
    }

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
            Err(OrderError::Filter(FilterError::PriceTooLow))
        );
        let price = QuoteCurrency::new(1001, 0);
        assert_eq!(
            filter.validate_limit_price(price, mark_price),
            Err(OrderError::Filter(FilterError::PriceTooHigh))
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
            Err(OrderError::Filter(FilterError::PriceStepSize {
                price: "100.05 Quote".to_string(),
                step_size: "0.10 Quote".to_string(),
            }))
        );
    }

    #[test]
    fn size_of_price_filter() {
        assert_eq!(std::mem::size_of::<PriceFilter<i64, 5>>(), 56);
    }

    #[test]
    fn price_filter_display() {
        let filter = PriceFilter::<i64, 1>::default();
        assert_eq!(
            &filter.to_string(),
            "PriceFilter( min_price: None, max_price: None, tick_size: 1.0 Quote, multiplier_up: 2.0, multiplier_down: 0.0 )",
        );
    }
}
