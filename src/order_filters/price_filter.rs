use malachite::Rational;

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
    /// (order.limit_price - min_price) % tick_size == 0
    pub tick_size: QuoteCurrency,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price <= mark_price * multiplier_up
    pub multiplier_up: Rational,

    /// Defines valid ranges for the order price relative to the mark price
    /// To pass this filter,
    /// order.limit_price >= mark_price * multiplier_down
    pub multiplier_down: Rational,
}

impl PriceFilter {
    /// check if an `Order` is valid
    pub(crate) fn validate_order<S>(
        &self,
        order: &Order<S>,
        mark_price: &QuoteCurrency,
    ) -> Result<(), OrderError>
    where
        S: Currency,
    {
        match order.limit_price() {
            Some(limit_price) => {
                if limit_price < &self.min_price && self.min_price != QuoteCurrency::new_zero() {
                    return Err(OrderError::LimitPriceTooLow);
                }
                if limit_price > &self.max_price && self.max_price != QuoteCurrency::new_zero() {
                    return Err(OrderError::LimitPriceTooHigh);
                }
                // TODO:
                // if ((limit_price - self.min_price) % self.tick_size) !=
                // QuoteCurrency::new_zero() {     return
                // Err(OrderError::InvalidOrderPriceStepSize); }
                if limit_price > &(mark_price * self.multiplier_up) && self.multiplier_up != 0.0 {
                    return Err(OrderError::LimitPriceTooHigh);
                }
                if limit_price < &(mark_price * self.multiplier_down) && self.multiplier_down != 0.0
                {
                    return Err(OrderError::LimitPriceTooLow);
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, quote, BaseCurrency, Side};

    /// Convert `f64` into `Rational`
    ///
    /// # Panics:
    /// If conversion fails
    fn rational_from_f64(val: f64) -> Rational {
        Rational::try_from_float_simplest(val).expect("Unable to get Rational from float")
    }

    #[test]
    fn price_filter() {
        let filter = PriceFilter {
            min_price: quote!(0.1),
            max_price: quote!(1000.0),
            tick_size: quote!(0.1),
            multiplier_up: rational_from_f64(1.2),
            multiplier_down: rational_from_f64(0.8),
        };
        let mark_price = quote!(100.0);

        let order = Order::market(Side::Buy, base!(0.1)).unwrap();
        filter.validate_order(&order, mark_price).unwrap();

        let order = Order::market(Side::Sell, base!(0.1)).unwrap();
        filter.validate_order(&order, mark_price).unwrap();

        let order = Order::limit(Side::Buy, quote!(99.0), base!(0.1)).unwrap();
        filter.validate_order(&order, mark_price).unwrap();

        let order = Order::limit(Side::Sell, quote!(99.0), base!(0.1)).unwrap();
        filter.validate_order(&order, mark_price).unwrap();

        let order = Order::limit(Side::Buy, quote!(0.05), base!(0.1)).unwrap();
        assert!(filter.validate_order(&order, mark_price).is_err());

        let order = Order::limit(Side::Buy, quote!(1001.0), base!(0.1)).unwrap();
        assert!(filter.validate_order(&order, mark_price).is_err());

        let order = Order::limit(Side::Buy, quote!(100.05), base!(0.1)).unwrap();
        assert!(filter.validate_order(&order, mark_price).is_err());

        // TODO: more
        todo!()
    }
}
