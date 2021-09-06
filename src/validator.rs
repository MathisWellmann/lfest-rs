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
        debug_assert!(bid > 0.0);
        debug_assert!(ask > 0.0);
        debug_assert!(bid <= ask);

        self.bid = bid;
        self.ask = ask;
    }

    /// Check if order is valid and passes risk check
    /// # Arguments
    /// - order to validate
    /// - account that submitted this order
    /// # Returns
    /// Either an OrderError if Order is invalid or (debit, credit) of order
    #[must_use]
    pub(crate) fn validate(&self, o: &Order, acc: &Account) -> Result<(f64, f64), OrderError> {
        match o.order_type() {
            OrderType::Market => self.validate_market_order(o, acc),
            OrderType::Limit => self.validate_limit_order(o, acc),
        }
    }

    /// Check if market order is correct
    #[must_use]
    fn validate_market_order(&self, o: &Order, acc: &Account) -> Result<(f64, f64), OrderError> {
        let (debit, credit) = self.order_cost_market(o, acc);
        debug!("validate_market_order debit: {}, credit: {}", debit, credit);

        if credit > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }

        Ok((debit, credit))
    }

    /// Check if a limit order is correct
    #[must_use]
    fn validate_limit_order(&self, o: &Order, acc: &Account) -> Result<(f64, f64), OrderError> {
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

        let (debit, credit) = self.limit_order_margin_cost(o, acc);

        debug!("validate_limit_order credit: {}, ab: {}, debit: {}", credit, acc.margin().available_balance(), debit);
        if credit > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }

        Ok((debit, credit))
    }

    /// Compute the order cost of a market order
    /// # Returns
    /// debited and credited account balance delta
    #[must_use]
    fn order_cost_market(&self, order: &Order, acc: &Account) -> (f64, f64) {
        debug!(
            "order_cost_market: order: {:?},\nacc.position: {:?}",
            order,
            acc.position()
        );

        let pos_size = acc.position().size();

        let (mut debit, mut credit) = if pos_size == 0.0 {
            match order.side() {
                Side::Buy => (min(order.size(), acc.open_limit_sell_size()), order.size()),
                Side::Sell => (min(order.size(), acc.open_limit_buy_size()), order.size()),
            }
        } else if pos_size > 0.0 {
            match order.side() {
                Side::Buy => (0.0, order.size()),
                Side::Sell => (
                    min(order.size(), acc.position().size()),
                    max(0.0, order.size() - acc.position().size()),
                ),
            }
        } else {
            match order.side() {
                Side::Buy => (
                    min(order.size(), acc.position().size().abs()),
                    max(0.0, order.size() - acc.position().size().abs()),
                ),
                Side::Sell => (0.0, order.size()),
            }
        };
        debit /= acc.position().leverage();
        credit /= acc.position().leverage();

        let mut fee: f64 = order.size() * self.fee_taker;

        let price = match order.side() {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        match self.futures_type {
            FuturesTypes::Linear => {
                // the values fee, debit and credit have to be converted from denoted in BASE currency
                // to being denoted in QUOTE currency
                fee *= price;
                debit *= price;
                credit *= price;
            }
            FuturesTypes::Inverse => {
                // the values fee, debit and credit have to be converted from denoted in QUOTE currency
                // to being denoted in BASE currency
                fee /= price;
                debit /= price;
                credit /= price;
            }
        }

        (debit, credit + fee)
    }

    /// Compute the order cost of a limit order
    /// # Returns
    /// debited and credited account balance delta
    #[must_use]
    fn limit_order_margin_cost(&self, order: &Order, acc: &Account) -> (f64, f64) {
        let (mut debit, mut credit) = match order.side() {
            Side::Buy => (
                min(
                    order.size(), // - acc.open_limit_sell_size(),
                    acc.open_limit_sell_size() + min(acc.position().size(), 0.0).abs(),
                ),
                max(
                    0.0,
                    order.size()
                        - min(acc.position().size(), 0.0).abs(),
                ),
            ),
            Side::Sell => (
                min(
                    order.size(), // - acc.open_limit_buy_size(),
                    acc.open_limit_buy_size() + max(acc.position().size(), 0.0),
                ),
                max(
                    0.0,
                    order.size() - max(acc.position().size(), 0.0),
                ),
            ),
        };
        debit /= acc.position().leverage();
        credit /= acc.position().leverage();

        let mut fee = self.fee_maker * order.size();

        let price = order.limit_price().unwrap();
        match self.futures_type {
            FuturesTypes::Linear => {
                // the values fee, debit and credit have to be converted from denoted in BASE currency
                // to being denoted in QUOTE currency
                fee *= price;
                debit *= price;
                credit *= price;
            }
            FuturesTypes::Inverse => {
                // the values fee, debit and credit have to be converted from denoted in QUOTE currency
                // to being denoted in BASE currency
                fee /= price;
                debit /= price;
                credit /= price;
            }
        }

        debug!(
            "limit_order_margin_cost: order: {:?}, acc.position: {:?}, olss: {}, osbs: {}, debit: {}, credit: {}",
            order,
            acc.position(),
            acc.open_limit_sell_size(),
            acc.open_limit_buy_size(),
            debit,
            credit,
        );

        (debit, credit + fee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FuturesTypes, round};

    #[test]
    fn validate_inverse_futures_market_order_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for leverage in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let mut acc = Account::new(leverage, 1.0, futures_type);

            let o = Order::market(Side::Buy, 40.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 105.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 40.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

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
            debug!("testing with leverage: {}", leverage);

            // with long position
            let mut acc = Account::new(leverage, 1.0, futures_type);
            acc.change_position(Side::Buy, 50.0 * leverage, 101.0);

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
            debug!("leverage: {}", leverage);

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
            debug!("leverage: {}", leverage);

            // with buy limit order
            let mut acc = Account::new(leverage, 1.0, futures_type);
            let o = Order::limit(Side::Buy, 100.0, 50.0 * leverage).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);

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
            let o = Order::limit(Side::Sell, 100.0, 50.0 * leverage).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);

            let o = Order::market(Side::Buy, 99.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Buy, 101.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());

            let o = Order::market(Side::Sell, 49.0 * leverage).unwrap();
            validator.validate_market_order(&o, &acc).unwrap();

            let o = Order::market(Side::Sell, 51.0 * leverage).unwrap();
            assert!(validator.validate_market_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let acc = Account::new(l, 1.0, futures_type);

            let o = Order::limit(Side::Buy, 100.0, 99.0 * l).unwrap();
            validator.validate_limit_order(&o, &acc).unwrap();

            let o = Order::limit(Side::Buy, 100.0, 101.0 * l).unwrap();
            assert!(validator.validate_limit_order(&o, &acc).is_err());

            let o = Order::limit(Side::Sell, 101.0, 99.0 * l).unwrap();
            validator.validate_limit_order(&o, &acc).unwrap();

            let o = Order::limit(Side::Sell, 101.0, 101.0 * l).unwrap();
            assert!(validator.validate_limit_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            // with long position
            let mut acc = Account::new(l, 1.0, futures_type);
            acc.change_position(Side::Buy, 50.0 * l, 101.0);

            let o = Order::limit(Side::Buy, 100.0, 49.0 * l).unwrap();
            validator.validate_limit_order(&o, &acc).unwrap();

            let o = Order::limit(Side::Buy, 100.0, 51.0 * l).unwrap();
            assert!(validator.validate_limit_order(&o, &acc).is_err());

            let o = Order::limit(Side::Sell, 101.0, 149.0 * l).unwrap();
            validator.validate_limit_order(&o, &acc).unwrap();

            let o = Order::limit(Side::Sell, 101.0, 151.0 * l).unwrap();
            assert!(validator.validate_limit_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            // with short position
            let mut acc = Account::new(l, 1.0, futures_type);
            acc.change_position(Side::Sell, 50.0 * l, 100.0);

            let o = Order::limit(Side::Buy, 100.0, 149.0 * l).unwrap();
            validator.validate_limit_order(&o, &acc).unwrap();

            let o = Order::limit(Side::Buy, 100.0, 151.0 * l).unwrap();
            assert!(validator.validate_limit_order(&o, &acc).is_err());

            let o = Order::limit(Side::Sell, 101.0, 49.0 * l).unwrap();
            validator.validate_limit_order(&o, &acc).unwrap();

            let o = Order::limit(Side::Sell, 101.0, 51.0 * l).unwrap();
            assert!(validator.validate_limit_order(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_open_buy_orders() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            // with open buy order
            let mut acc = Account::new(l, 1.0, futures_type);
            let o = Order::limit(Side::Buy, 100.0, 50.0 * l).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);

            let o = Order::limit(Side::Buy, 100.0, 49.0 * l).unwrap();
            let _ = validator.validate(&o, &acc).unwrap();

            let o = Order::limit(Side::Buy, 100.0, 51.0 * l).unwrap();
            assert!(validator.validate(&o, &acc).is_err());

            let o = Order::limit(Side::Sell, 101.0, 99.0 * l).unwrap();
            let _ = validator.validate(&o, &acc).unwrap();

            let o = Order::limit(Side::Sell, 101.0, 101.0 * l).unwrap();
            assert!(validator.validate(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_open_sell_orders() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            // with open sell order
            let mut acc = Account::new(l, 1.0, futures_type);
            let o = Order::limit(Side::Sell, 101.0, 50.0 * l).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);

            let o = Order::limit(Side::Buy, 100.0, 99.0 * l).unwrap();
            validator.validate(&o, &acc).unwrap();

            let o = Order::limit(Side::Buy, 100.0, 101.0 * l).unwrap();
            assert!(validator.validate(&o, &acc).is_err());

            let o = Order::limit(Side::Sell, 101.0, 49.0 * l).unwrap();
            validator.validate(&o, &acc).unwrap();

            let o = Order::limit(Side::Sell, 101.0, 51.0 * l).unwrap();
            assert!(validator.validate(&o, &acc).is_err());
        }
    }

    #[test]
    fn validate_inverse_futures_limit_order_with_open_orders_mixed() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            // with mixed limit orders
            let mut acc = Account::new(l, 1.0, futures_type);
            let o = Order::limit(Side::Buy, 100.0, 50.0 * l).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);
            let o = Order::limit(Side::Sell, 101.0, 50.0 * l).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);
            let fee = (0.0002 * 50.0 * l / 100.0) + (0.0002 * 50.0 * l / 100.0);
            assert_eq!(round(acc.margin().order_margin(), 4), 0.5 + fee);

            let o = Order::limit(Side::Buy, 100.0, 49.0 * l).unwrap();
            validator.validate(&o, &acc).unwrap();

            let o = Order::limit(Side::Buy, 100.0, 51.0 * l).unwrap();
            assert!(validator.validate(&o, &acc).is_err());

            let o = Order::limit(Side::Sell, 101.0, 49.0 * l).unwrap();
            validator.validate(&o, &acc).unwrap();

            let o = Order::limit(Side::Sell, 101.0, 51.0 * l).unwrap();
            assert!(validator.validate(&o, &acc).is_err());
        }
    }

    #[test]
    #[should_panic]
    // basically you should not be able to endlessly add both buy and sell orders
    fn validate_inverse_futures_limit_order_with_open_orders_mixed_panic() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut validator = Validator::new(0.0002, 0.0006, futures_type);
        validator.update(100.0, 101.0);
        let mut acc = Account::new(1.0, 1.0, futures_type);

        for i in 0..100 {
            // with mixed limit orders
            let o = Order::limit(Side::Buy, 100.0, 50.0).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);
            let o = Order::limit(Side::Sell, 101.0, 50.0).unwrap();
            let (debit, credit) = validator.validate(&o, &acc).unwrap();
            acc.append_limit_order(o, debit, credit);
        }
        debug!("final account olbs: {}, olss: {:?}", acc.open_limit_buy_size(), acc.open_limit_sell_size());
    }
}
