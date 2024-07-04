use fpdec::{Dec, Decimal};
use getset::CopyGetters;

use crate::{
    prelude::{ConfigError, FilterError, OrderError},
    quote,
    types::{Currency, LimitOrder, NewOrder, QuoteCurrency},
};

/// The `PriceFilter` defines the price rules for a symbol
#[derive(Debug, Clone, CopyGetters)]
pub struct PriceFilter {
    /// Defines the optional minimum price allowed.
    #[getset(get_copy = "pub")]
    min_price: Option<QuoteCurrency>,

    /// Defines the optional maximum price allowed.
    #[getset(get_copy = "pub")]
    max_price: Option<QuoteCurrency>,

    /// Defines the intervals that a price can be increased / decreased by.
    /// For the filter to pass,
    /// (order.limit_price - min_price) % tick_size == 0
    #[getset(get_copy = "pub")]
    tick_size: QuoteCurrency,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price <= mark_price * multiplier_up
    #[getset(get_copy = "pub")]
    multiplier_up: Decimal,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price >= mark_price * multiplier_down
    #[getset(get_copy = "pub")]
    multiplier_down: Decimal,
}

impl Default for PriceFilter {
    fn default() -> Self {
        Self {
            min_price: None,
            max_price: None,
            tick_size: quote!(1),
            multiplier_up: Decimal::TWO,
            multiplier_down: Decimal::ZERO,
        }
    }
}

impl PriceFilter {
    /// Create a new `PriceFilter`.
    pub fn new(
        min_price: Option<QuoteCurrency>,
        max_price: Option<QuoteCurrency>,
        tick_size: QuoteCurrency,
        multiplier_up: Decimal,
        multiplier_down: Decimal,
    ) -> Result<Self, ConfigError> {
        if let Some(min_qty) = min_price {
            if (min_qty % tick_size) != quote!(0) {
                return Err(ConfigError::InvalidMinPrice);
            }
        }

        if tick_size == quote!(0) {
            return Err(ConfigError::InvalidTickSize);
        }

        if multiplier_up <= Dec!(1) {
            return Err(ConfigError::InvalidUpMultiplier);
        }

        if multiplier_down >= Dec!(1) {
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
    pub(crate) fn validate_limit_order<Q, UserOrderId>(
        &self,
        order: &LimitOrder<Q, UserOrderId, NewOrder>,
        mark_price: QuoteCurrency,
    ) -> Result<(), OrderError>
    where
        Q: Currency,
        UserOrderId: Clone,
    {
        if order.limit_price() <= quote!(0) {
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
            quote!(0)
        };

        if ((order.limit_price() - min_price) % self.tick_size) != QuoteCurrency::new_zero() {
            return Err(OrderError::InvalidOrderPriceStepSize);
        }
        if order.limit_price() > mark_price * self.multiplier_up
            && self.multiplier_up != Decimal::ZERO
        {
            return Err(OrderError::LimitPriceAboveMultiple);
        }
        if order.limit_price() < mark_price * self.multiplier_down
            && self.multiplier_down != Decimal::ZERO
        {
            return Err(OrderError::LimitPriceBelowMultiple);
        }
        Ok(())
    }
}

/// Errors if there is no bid-ask spread
pub(crate) fn enforce_bid_ask_spread(
    bid: QuoteCurrency,
    ask: QuoteCurrency,
) -> Result<(), FilterError> {
    if bid >= ask {
        return Err(FilterError::InvalidMarketUpdateBidAskSpread);
    }
    Ok(())
}

/// Make sure the price is not too low
/// Disabled if `min_price` == 0
pub(crate) fn enforce_min_price(
    min_price: Option<QuoteCurrency>,
    price: QuoteCurrency,
) -> Result<(), FilterError> {
    if let Some(min_price) = min_price {
        if price < min_price && min_price != quote!(0) {
            return Err(FilterError::MarketUpdatePriceTooLow);
        }
    }
    Ok(())
}

/// Make sure the price is not too high
/// Disabled if `max_price` == 0
pub(crate) fn enforce_max_price(
    max_price: Option<QuoteCurrency>,
    price: QuoteCurrency,
) -> Result<(), FilterError> {
    if let Some(max_price) = max_price {
        if price > max_price && max_price != quote!(0) {
            return Err(FilterError::MarketUpdatePriceTooHigh);
        }
    }
    Ok(())
}

/// Make sure the price conforms to the step size
pub(crate) fn enforce_step_size(
    step_size: QuoteCurrency,
    price: QuoteCurrency,
) -> Result<(), FilterError> {
    if (price % step_size) != QuoteCurrency::new_zero() {
        return Err(FilterError::MarketUpdatePriceStepSize { price, step_size });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use fpdec::Dec;

    use super::*;
    use crate::prelude::*;

    #[test]
    fn price_filter() {
        let filter = PriceFilter {
            min_price: Some(quote!(0.1)),
            max_price: Some(quote!(1000.0)),
            tick_size: quote!(0.1),
            multiplier_up: Dec!(1.2),
            multiplier_down: Dec!(0.8),
        };
        let mark_price = quote!(100.0);

        // Some passing orders
        let order = LimitOrder::new(Side::Buy, quote!(99.0), base!(0.1)).unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();
        let order = LimitOrder::new(Side::Sell, quote!(99.0), base!(0.1)).unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();

        // beyond max and min
        let order = LimitOrder::new(Side::Buy, quote!(0.05), base!(0.1)).unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceBelowMin)
        );
        let order = LimitOrder::new(Side::Buy, quote!(1001), base!(0.1)).unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceAboveMax)
        );

        // Test upper price band
        let order = LimitOrder::new(Side::Buy, quote!(120), base!(0.1)).unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();
        let order = LimitOrder::new(Side::Buy, quote!(121), base!(0.1)).unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceAboveMultiple)
        );

        // Test lower price band
        let order = LimitOrder::new(Side::Buy, quote!(80), base!(0.1)).unwrap();
        filter.validate_limit_order(&order, mark_price).unwrap();
        let order = LimitOrder::new(Side::Buy, quote!(79), base!(0.1)).unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::LimitPriceBelowMultiple)
        );

        // Test step size
        let order = LimitOrder::new(Side::Buy, quote!(100.05), base!(0.1)).unwrap();
        assert_eq!(
            filter.validate_limit_order(&order, mark_price),
            Err(OrderError::InvalidOrderPriceStepSize)
        );
    }
}
