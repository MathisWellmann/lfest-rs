use crate::{max, min, Account, FuturesTypes, Order, OrderError, OrderType, Side};

#[derive(Clone, Debug, Default)]
/// Used for validating orders
pub(crate) struct Validator {
    fee_maker: f64,
    fee_taker: f64,
    bid: f64,
    ask: f64,
    futures_type: FuturesTypes,
}

impl Validator {
    /// Create a new Validator with a given fee maker and taker
    #[inline]
    pub(crate) fn new(fee_maker: f64, fee_taker: f64, futures_type: FuturesTypes) -> Self {
        Self {
            fee_maker,
            fee_taker,
            bid: 0.0,
            ask: 0.0,
            futures_type,
        }
    }

    /// update the state with newest information
    #[inline]
    pub(crate) fn update(&mut self, bid: f64, ask: f64) {
        self.bid = bid;
        self.ask = ask;
    }

    /// Check if order is valid and passes risk check
    #[must_use]
    pub(crate) fn validate(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        match o.order_type() {
            OrderType::Market => self.validate_market_order(o, acc),
            OrderType::Limit => self.validate_limit_order(o, acc),
        }
    }

    /// Check if market order is correct
    #[must_use]
    fn validate_market_order(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        let (debit, credit) = self.order_cost_market(o, acc);
        debug!("validate_market_order debit: {}, credit: {}", debit, credit);
        if credit > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    /// Check if a limit order is correct
    #[must_use]
    fn validate_limit_order(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        // validate order price
        match o.side() {
            Side::Buy => {
                if o.limit_price().unwrap() > self.ask {
                    return Err(OrderError::InvalidLimitPrice);
                }
            }
            Side::Sell => {
                if o.limit_price().unwrap() < self.bid {
                    return Err(OrderError::InvalidLimitPrice);
                }
            }
        }

        let (debit, credit) = self.order_cost_limit(o, acc);
        if credit > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }
        Ok(())
    }

    /// Compute the order cost of a market order
    /// using hedged volume
    /// # Returns
    /// debited and credited account balance delta
    #[must_use]
    fn order_cost_market(&self, order: &Order, acc: &Account) -> (f64, f64) {
        let hedged_size = match order.side() {
            Side::Buy => max(0.0, min(order.size(), -acc.position().size())),
            Side::Sell => max(0.0, min(order.size(), acc.position().size())),
        };
        let unhedged_size = order.size() - hedged_size;

        let price: f64 = match order.side() {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        // include fee in order cost
        let fee_bps: f64 = match order.order_type() {
            OrderType::Market => self.fee_taker,
            OrderType::Limit => self.fee_maker,
        };
        let mut fee: f64 = fee_bps * order.size();

        let mut debit: f64 = hedged_size * (1.0 / acc.position().leverage());
        let mut credit: f64 = unhedged_size * (1.0 / acc.position().leverage());
        debug_assert!(debit.is_finite());
        debug_assert!(credit.is_finite());

        match self.futures_type {
            FuturesTypes::Linear => {
                fee *= price;
                debit *= price;
                credit *= price;
            }
            FuturesTypes::Inverse => {
                fee /= price;
                debit /= price;
                credit /= price;
            }
        }
        debug!(
            "order_cost_market: debit: {}, credit: {}, fee: {}",
            debit, credit, fee
        );

        (debit, credit + fee)
    }

    /// Compute the order cost of a passively sitting order such as limit and stop orders
    /// # Returns
    /// debited and credited account balance delta
    #[must_use]
    fn order_cost_limit(&self, order: &Order, acc: &Account) -> (f64, f64) {
        debug!(
            "order_cost_limit: order: {:?}, acc.position: {:?}",
            order,
            acc.position()
        );

        let fee = self.fee_maker * order.size();

        let pos_size = acc.position().size();
        if pos_size == 0.0 {
            (0.0, order.size() + fee)
        } else if pos_size > 0.0 {
            match order.side() {
                Side::Buy => (0.0, order.size() + fee),
                Side::Sell => {
                    let debit = min(order.size(), pos_size);
                    (debit, max(0.0, order.size() - pos_size) + fee)
                }
            }
        } else {
            match order.side() {
                Side::Buy => {
                    let debit = min(order.size(), pos_size.abs());
                    (debit, max(0.0, pos_size.abs() - order.size()) + fee)
                }
                Side::Sell => (0.0, order.size() + fee),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FuturesTypes;

    #[test]
    fn validate_inverse_market_order_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for leverage in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let mut acc = Account::new(leverage, 1.0, futures_type);

            let o = Order::market(Side::Buy, 40.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Sell, 40.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 105.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 105.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_market_order_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for leverage in [1.0, 2.0, 3.0, 4.0, 5.0] {
            // with long position
            let mut acc = Account::new(leverage, 1.0, futures_type);
            acc.change_position(Side::Buy, 50.0 * leverage, 100.0);

            let o = Order::market(Side::Buy, 49.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 51.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 149.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Sell, 151.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_market_order_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for leverage in [1.0, 2.0, 3.0, 4.0, 5.0] {
            // with short position
            let mut acc = Account::new(leverage, 1.0, futures_type);
            acc.change_position(Side::Sell, 50.0 * leverage, 100.0);

            let o = Order::market(Side::Buy, 149.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 151.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 49.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Sell, 51.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_market_order_with_open_orders() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for leverage in [1.0, 2.0, 3.0, 4.0, 5.0] {
            // with buy limit order
            let mut acc = Account::new(leverage, 1.0, futures_type);
            acc.append_order(Order::limit(Side::Buy, 100.0, 50.0 * leverage).unwrap());

            let o = Order::market(Side::Buy, 49.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 51.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 99.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Sell, 101.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            // with sell limit order
            let mut acc = Account::new(leverage, 1.0, futures_type);
            acc.append_order(Order::limit(Side::Sell, 100.0, 50.0 * leverage).unwrap());

            let o = Order::market(Side::Buy, 99.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 101.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 99.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Sell, 101.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        let acc = Account::new(1.0, 1.0, futures_type);

        let o = Order::limit(Side::Buy, 100.0, 99.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        let o = Order::limit(Side::Buy, 100.0, 101.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());

        let o = Order::limit(Side::Sell, 101.0, 99.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        let o = Order::limit(Side::Sell, 101.0, 101.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        // with long position
        let mut acc = Account::new(1.0, 1.0, futures_type);
        acc.change_position(Side::Buy, 50.0, 101.0);

        let o = Order::limit(Side::Buy, 100.0, 49.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        let o = Order::limit(Side::Buy, 100.0, 51.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());

        let o = Order::limit(Side::Sell, 101.0, 149.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        let o = Order::limit(Side::Sell, 101.0, 151.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        // with short position
        let mut acc = Account::new(1.0, 1.0, futures_type);
        acc.change_position(Side::Sell, 50.0, 100.0);

        let o = Order::limit(Side::Buy, 100.0, 49.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        let o = Order::limit(Side::Buy, 100.0, 51.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());

        let o = Order::limit(Side::Sell, 101.0, 149.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        let o = Order::limit(Side::Sell, 101.0, 151.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_open_orders() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test Validator for limit orders with an account that has open orders

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        // with open orders
        let mut acc = Account::new(1.0, 1.0, futures_type);
        acc.append_order(Order::limit(Side::Buy, 90.0, 45.0).unwrap());
    }
}
