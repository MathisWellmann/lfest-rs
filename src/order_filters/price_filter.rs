use const_decimal::Decimal;
use getset::CopyGetters;
use num_traits::{One, Zero};

use crate::{
    prelude::{
        BasisPointFrac, ConfigError, CurrencyMarker, FilterError, Mon, OrderError, QuoteCurrency,
    },
    types::{LimitOrder, NewOrder},
};

/// The `PriceFilter` defines the price rules for a symbol
#[derive(Debug, Clone, CopyGetters)]
pub struct PriceFilter<I, const DB: u8, const DQ: u8>
where
    I: Mon<DB> + Mon<DQ>,
{
    /// Defines the optional minimum price allowed.
    #[getset(get_copy = "pub")]
    min_price: Option<QuoteCurrency<I, DB, DQ>>,

    /// Defines the optional maximum price allowed.
    #[getset(get_copy = "pub")]
    max_price: Option<QuoteCurrency<I, DB, DQ>>,

    /// Defines the intervals that a price can be increased / decreased by.
    /// For the filter to pass,
    /// (order.limit_price - min_price) % tick_size == 0
    #[getset(get_copy = "pub")]
    tick_size: QuoteCurrency<I, DB, DQ>,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price <= mark_price * multiplier_up
    #[getset(get_copy = "pub")]
    multiplier_up: BasisPointFrac,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price >= mark_price * multiplier_down
    #[getset(get_copy = "pub")]
    multiplier_down: BasisPointFrac,
}

impl<I, const DB: u8, const DQ: u8> Default for PriceFilter<I, DB, DQ>
where
    I: Mon<DB> + Mon<DQ>,
{
    fn default() -> Self {
        Self {
            min_price: None,
            max_price: None,
            tick_size: QuoteCurrency::from(Decimal::one()),
            multiplier_up: BasisPointFrac::from(Decimal::TWO),
            multiplier_down: BasisPointFrac::zero(),
        }
    }
}

impl<I, const DB: u8, const DQ: u8> PriceFilter<I, DB, DQ>
where
    I: Mon<DB> + Mon<DQ>,
{
    /// Create a new `PriceFilter`.
    pub fn new(
        min_price: Option<QuoteCurrency<I, DB, DQ>>,
        max_price: Option<QuoteCurrency<I, DB, DQ>>,
        tick_size: QuoteCurrency<I, DB, DQ>,
        multiplier_up: BasisPointFrac,
        multiplier_down: BasisPointFrac,
    ) -> Result<Self, ConfigError> {
        if let Some(min_qty) = min_price {
            if (min_qty % tick_size) != QuoteCurrency::zero() {
                return Err(ConfigError::InvalidMinPrice);
            }
        }

        if tick_size.is_zero() {
            return Err(ConfigError::InvalidTickSize);
        }

        if multiplier_up <= BasisPointFrac::one() {
            return Err(ConfigError::InvalidUpMultiplier);
        }

        if multiplier_down >= BasisPointFrac::one() {
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
    pub(crate) fn validate_limit_order<BaseOrQuote, UserOrderId>(
        &self,
        order: &LimitOrder<I, DB, DQ, BaseOrQuote, UserOrderId, NewOrder>,
        mark_price: QuoteCurrency<I, DB, DQ>,
    ) -> Result<(), OrderError<I, DB, DQ>>
    where
        BaseOrQuote: CurrencyMarker<I, DB, DQ>,
        UserOrderId: Clone,
    {
        if order.limit_price() <= QuoteCurrency::zero() {
            return Err(OrderError::LimitPriceBelowMin);
        }

        if let Some(max_price) = self.max_price {
            if order.limit_price() > max_price {
                return Err(OrderError::LimitPriceAboveMax);
            }
        }

        let min_price = if let Some(min_price) = self.min_price {
            if order.limit_price() < min_price {
                return Err(OrderError::LimitPriceBelowMin);
            }
            min_price
        } else {
            QuoteCurrency::zero()
        };

        if ((order.limit_price() - min_price) % self.tick_size) != QuoteCurrency::zero() {
            return Err(OrderError::InvalidOrderPriceStepSize);
        }
        if order.limit_price() > mark_price * self.multiplier_up
            && self.multiplier_up != BasisPointFrac::zero()
        {
            return Err(OrderError::LimitPriceAboveMultiple);
        }
        if order.limit_price() < mark_price * self.multiplier_down
            && self.multiplier_down != BasisPointFrac::zero()
        {
            return Err(OrderError::LimitPriceBelowMultiple);
        }
        Ok(())
    }
}

/// Errors if there is no bid-ask spread
pub(crate) fn enforce_bid_ask_spread<I, const DB: u8, const DQ: u8>(
    bid: QuoteCurrency<I, DB, DQ>,
    ask: QuoteCurrency<I, DB, DQ>,
) -> Result<(), FilterError<I, DB, DQ>>
where
    I: Mon<DB> + Mon<DQ>,
{
    if bid >= ask {
        return Err(FilterError::InvalidMarketUpdateBidAskSpread);
    }
    Ok(())
}

/// Make sure the price is not too low
/// Disabled if `min_price` == 0
pub(crate) fn enforce_min_price<I, const DB: u8, const DQ: u8>(
    min_price: Option<QuoteCurrency<I, DB, DQ>>,
    price: QuoteCurrency<I, DB, DQ>,
) -> Result<(), FilterError<I, DB, DQ>>
where
    I: Mon<DB> + Mon<DQ>,
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
pub(crate) fn enforce_max_price<I, const DB: u8, const DQ: u8>(
    max_price: Option<QuoteCurrency<I, DB, DQ>>,
    price: QuoteCurrency<I, DB, DQ>,
) -> Result<(), FilterError<I, DB, DQ>>
where
    I: Mon<DB> + Mon<DQ>,
{
    if let Some(max_price) = max_price {
        if price > max_price && max_price != QuoteCurrency::zero() {
            return Err(FilterError::MarketUpdatePriceTooHigh);
        }
    }
    Ok(())
}

/// Make sure the price conforms to the step size
pub(crate) fn enforce_step_size<I, const DB: u8, const DQ: u8>(
    step_size: QuoteCurrency<I, DB, DQ>,
    price: QuoteCurrency<I, DB, DQ>,
) -> Result<(), FilterError<I, DB, DQ>>
where
    I: Mon<DB> + Mon<DQ>,
{
    if (price % step_size) != QuoteCurrency::zero() {
        return Err(FilterError::MarketUpdatePriceStepSize { price, step_size });
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
            min_price: Some(QuoteCurrency::<i32, 4, 2>::new(1, 1)),
            max_price: Some(QuoteCurrency::new(1000, 0)),
            tick_size: QuoteCurrency::new(1, 1),
            multiplier_up: BasisPointFrac::from(Decimal::try_from_scaled(12, 1).unwrap()),
            multiplier_down: BasisPointFrac::from(Decimal::try_from_scaled(8, 1).unwrap()),
        };
        let mark_price = QuoteCurrency::new(100, 0);

        // Some passing orders
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();
        let order = LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();

        // beyond max and min
        let order =
            LimitOrder::new(Side::Buy, QuoteCurrency::new(5, 2), BaseCurrency::new(1, 1)).unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceBelowMin)
        );
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(1001, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceAboveMax)
        );

        // Test upper price band
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(120, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(121, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceAboveMultiple)
        );

        // Test lower price band
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(80, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(79, 0),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceBelowMultiple)
        );

        // Test step size
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(10005, 2),
            BaseCurrency::new(1, 1),
        )
        .unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::InvalidOrderPriceStepSize)
        );
    }
}
