//! This module contains order filtering related code

use crate::{Currency, Order, OrderError, QuoteCurrency};

/// The `PriceFilter` defines the price rules for a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceFilter {
    /// Defines the minimum price allowed. Disabled if `min_price` == 0
    pub min_price: QuoteCurrency,

    /// Defines the maximum price allowed. Disabled if `max_price` == 0
    pub max_price: QuoteCurrency,

    /// Defines the intervals that a price can be increased / decreased by.
    /// For the filter to pass,
    /// (price - min_price) % tick_size == 0
    pub tick_size: QuoteCurrency,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.price <= mark_price * multiplier_up
    pub multiplier_up: f64,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.price >= mark_price * multiplier_down
    pub multiplier_down: f64,
}

/// The `SizeFilter` defines the quantity rules that each order needs to follow
/// The generic currency `S` is always the `PairedCurrency` of the margin
/// currency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityFilter<S>
where S: Currency
{
    /// Defines the minimum `quantity` of any order
    pub min_qty: S,

    /// Defines the maximum `quantity` of any order
    pub max_qty: S,

    /// Defines the intervals that a `quantity` can be increased / decreased by.
    /// For the filter to pass,
    /// (quantity - min_qty) % step_size == 0
    pub step_size: S,
}

impl PriceFilter {
    /// check if an `Order` is valid
    pub(crate) fn validate_order<S>(
        &self,
        order: &Order<S>,
        mark_price: QuoteCurrency,
    ) -> Result<(), OrderError>
    where
        S: Currency,
    {
        match order.limit_price() {
            Some(limit_price) => {
                if limit_price < self.min_price {
                    return Err(OrderError::LimitPriceTooLow);
                }
                if limit_price > self.max_price {
                    return Err(OrderError::LimitPriceTooHigh);
                }
                if (limit_price - self.min_price) % self.tick_size != QuoteCurrency::new_zero() {
                    return Err(OrderError::InvalidOrderPriceStepSize);
                }
                if limit_price > mark_price * self.multiplier_up.into() {
                    return Err(OrderError::LimitPriceTooHigh);
                }
                if limit_price < mark_price * self.multiplier_down.into() {
                    return Err(OrderError::LimitPriceTooLow);
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}
