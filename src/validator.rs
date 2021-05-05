use crate::{max, min, Account, Order, OrderError, OrderType, Side};

#[derive(Clone, Debug, Default)]
/// Used for validating orders
pub(crate) struct Validator {
    fee_maker: f64,
    fee_taker: f64,
    bid: f64,
    ask: f64,
}

impl Validator {
    /// Create a new Validator with a given fee maker and taker
    pub(crate) fn new(fee_maker: f64, fee_taker: f64) -> Self {
        Self {
            fee_maker,
            fee_taker,
            bid: 0.0,
            ask: 0.0,
        }
    }

    /// update the state with newest information
    pub(crate) fn update(&mut self, bid: f64, ask: f64) {
        self.bid = bid;
        self.ask = ask;
    }

    /// Check if order is valid and passes risk check
    pub(crate) fn validate(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        match o.order_type {
            OrderType::Market => self.validate_market_order(o, acc),
            OrderType::Limit => self.validate_limit_order(o, acc),
            OrderType::StopMarket => self.validate_stop_market_order(o, acc),
        }
    }

    /// Check if market order is correct
    fn validate_market_order(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        let price = match o.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let fee_base = self.fee_taker * o.size;
        let fee_quote = fee_base / price;

        let (debit, credit) = self.order_cost(o, acc);
        debug!("validate_market_order debit: {}, credit: {}", debit, credit);
        if credit + fee_quote > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    /// Check if a limit order is correct
    fn validate_limit_order(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        // validate order price
        match o.side {
            Side::Buy => {
                if o.limit_price > self.ask {
                    return Err(OrderError::InvalidLimitPrice);
                }
            }
            Side::Sell => {
                if o.limit_price < self.bid {
                    return Err(OrderError::InvalidLimitPrice);
                }
            }
        }
        let price = match o.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let fee_base = self.fee_taker * o.size;
        let fee_quote = fee_base / price;

        let (debit, credit) = self.order_cost(o, acc);
        if credit + fee_quote > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }
        Ok(())
    }

    /// Check if a stop market order is correct
    fn validate_stop_market_order(&self, o: &Order, acc: &Account) -> Result<(), OrderError> {
        match o.side {
            Side::Buy => {
                if o.trigger_price <= self.ask {
                    return Err(OrderError::InvalidTriggerPrice);
                }
            }
            Side::Sell => {
                if o.trigger_price >= self.bid {
                    return Err(OrderError::InvalidTriggerPrice);
                }
            }
        }
        let (debit, credit) = self.order_cost(o, acc);
        if credit > acc.margin().available_balance() + debit {
            return Err(OrderError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    /// Calculate the cost of order
    /// # Returns
    /// debited and credited account balance delta
    fn order_cost(&self, order: &Order, acc: &Account) -> (f64, f64) {
        match order.order_type {
            OrderType::Market => self.order_cost_market(order, acc),
            OrderType::Limit => self.order_cost_limit(order, acc),
            OrderType::StopMarket => self.order_cost_stop(order, acc),
        }
    }

    /// Compute the order cost of a market order
    /// using hedged volume
    /// # Returns
    /// debited and credited account balance delta
    fn order_cost_market(&self, order: &Order, acc: &Account) -> (f64, f64) {
        let hedged_size = match order.side {
            Side::Buy => max(0.0, min(order.size, -acc.position().size())),
            Side::Sell => max(0.0, min(order.size, acc.position().size()))
        };
        let unhedged_size = order.size - hedged_size;

        let price: f64 = match order.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        // include fee in order cost
        let fee: f64 = match order.order_type {
            OrderType::Market => self.fee_taker,
            OrderType::Limit => self.fee_maker,
            OrderType::StopMarket => self.fee_taker,
        };
        let fee_base: f64 = fee * order.size / price;

        let debit: f64 = hedged_size / price / acc.position().leverage();
        let credit: f64 = fee_base + (unhedged_size / price / acc.position().leverage());
        debug!("order_cost_market: debit: {}, credit: {}", debit, credit);

        (debit, credit)
    }

    /// Compute the order cost of a passively sitting order such as limit and stop orders
    /// # Returns
    /// debited and credited account balance delta
    fn order_cost_limit(&self, order: &Order, acc: &Account) -> (f64, f64) {
        let mut olbs = acc.open_limit_buy_size;
        let mut olss = acc.open_limit_sell_size;
        let mut osbs = acc.open_stop_buy_size;
        let mut osss = acc.open_stop_sell_size;
        match order.order_type {
            OrderType::Limit => match order.side {
                Side::Buy => olbs += order.size,
                Side::Sell => olss += order.size,
            },
            OrderType::StopMarket => match order.side {
                Side::Buy => osbs += order.size,
                Side::Sell => osss += order.size,
            },
            _ => panic!("market order should not be passed into this function!")
        }
        let open_sizes: [f64; 4] = [olbs, olss, osbs, osss];
        let mut max_idx: usize = 0;
        let mut max_size: f64 = acc.open_limit_buy_size;
        for (i, s) in open_sizes.iter().enumerate() {
            if *s > max_size {
                max_size = *s;
                max_idx = i;
            }
        }

        // direction of dominating open order side
        let (d, reference_price) = match max_idx {
            0 => (1.0, acc.min_limit_buy_price),
            1 => (-1.0, acc.max_limit_sell_price),
            2 => (1.0, acc.max_stop_buy_price),
            3 => (-1.0, acc.min_stop_sell_price),
            _ => panic!("any other value should not be possible"),
        };

        let order_price: f64 = match order.order_type {
            OrderType::Market => match order.side {
                Side::Buy => self.ask,
                Side::Sell => self.bid,
            },
            OrderType::Limit => order.limit_price,
            OrderType::StopMarket => order.trigger_price,
        };

        // include fee in order cost
        let fee: f64 = match order.order_type {
            OrderType::Market => self.fee_taker,
            OrderType::Limit => self.fee_maker,
            OrderType::StopMarket => self.fee_taker,
        };
        let fee_base: f64 = fee * max_size;

        // TODO: whats the debit for limit orders
        let debit: f64 = 0.0;
        let credit: f64 = fee_base
            + max(0.0, min(max_size, max_size + d * acc.position().size()))
            / order_price
            / acc.position().leverage();

        (debit, credit)
    }

    /// # Returns
    /// debited and credited account balance delta
    fn order_cost_stop(&self, order: &Order, acc: &Account) -> (f64, f64) {
        // TODO:
        (0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::round;

    #[test]
    fn validate_market_order_0() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // Test Validator with a fresh account

        let mut validator = Validator::new(0.0, 0.001);
        let bid: f64 = 1_000.0;
        let ask: f64 = 1_000.0;
        validator.update(bid, ask);

        let mut acc = Account::new(1.0, 1.0);

        // valid order
        let o = Order::market(Side::Buy, 400.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Sell, 400.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::market(Side::Buy, 1050.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());

        // invalid order
        let o = Order::market(Side::Sell, 1050.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_market_order_1() {
        if let Err(_) = pretty_env_logger::try_init() {}
        // test Validator with an account that has an open position

        let mut validator = Validator::new(0.0, 0.0);
        let bid: f64 = 100.0;
        let ask: f64 = 100.0;
        validator.update(bid, ask);

        // with long position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Buy, 50.0, 100.0);

        // valid order
        let o = Order::market(Side::Buy, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Sell, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::market(Side::Buy, 60.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());

        // valid order
        let o = Order::market(Side::Sell, 100.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Sell, 110.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // with short position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Sell, 50.0, 100.0);

        // valid order
        let o = Order::market(Side::Buy, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Sell, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::market(Side::Buy, 60.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Buy, 110.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::market(Side::Sell, 60.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_market_order_2() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test Validator with an account that has open orders

        let mut validator = Validator::new(0.0, 0.0);
        let bid: f64 = 100.0;
        let ask: f64 = 100.0;
        validator.update(bid, ask);

        // with buy limit order
        let mut acc = Account::new(1.0, 1.0);
        acc.append_order(Order::limit(Side::Buy, 100.0, 50.0).unwrap());

        // valid order
        let o = Order::market(Side::Buy, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Sell, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::market(Side::Buy, 60.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());

        // invalid order
        let o = Order::market(Side::Sell, 100.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());

        // with sell limit order
        let mut acc = Account::new(1.0, 1.0);
        acc.append_order(Order::limit(Side::Sell, 100.0, 50.0).unwrap());

        // valid order
        let o = Order::market(Side::Buy, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // valid order
        let o = Order::market(Side::Sell, 50.0).unwrap();
        validator.validate_market_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::market(Side::Sell, 60.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());

        // invalid order
        let o = Order::market(Side::Buy, 150.0).unwrap();
        assert!(validator.validate_market_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_limit_order_0() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test Validator for limit orders with a fresh account

        let mut validator = Validator::new(0.0, 0.001);
        validator.update(100.0, 100.0);

        let acc = Account::new(1.0, 1.0);

        // valid order
        let o = Order::limit(Side::Buy, 100.0, 50.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        // valid order
        let o = Order::limit(Side::Sell, 100.0, 50.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        // invalid order
        let o = Order::limit(Side::Buy, 100.0, 110.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());

        // invalid order
        let o = Order::limit(Side::Sell, 100.0, 110.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());

        // invalid order
        let o = Order::limit(Side::Buy, 110.0, 50.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());

        // invalid order
        let o = Order::limit(Side::Sell, 90.0, 50.0).unwrap();
        assert!(validator.validate_limit_order(&o, &acc).is_err());
    }

    #[test]
    fn validate_limit_order_1() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test Validator for limit orders with an account that has an open position

        let mut validator = Validator::new(0.0, 0.001);
        validator.update(100.0, 100.0);

        // with long position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Buy, 50.0, 100.0);

        // valid order
        let o = Order::limit(Side::Buy, 100.0, 50.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        // valid order
        let o = Order::limit(Side::Sell, 100.0, 50.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        // valid order
        let o = Order::limit(Side::Sell, 100.0, 150.0).unwrap();
        validator.validate_limit_order(&o, &acc).unwrap();

        // TODO: with short position
    }

    #[test]
    fn validate_limit_order_2() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test Validator for limit orders with an account that has open orders

        let mut validator = Validator::new(0.0, 0.001);
        validator.update(100.0, 100.0);

        // with long position
        let mut acc = Account::new(1.0, 1.0);
        acc.append_order(Order::limit(Side::Buy, 90.0, 45.0).unwrap());

        // TODO: validate limit order with long position


        let mut acc = Account::new(1.0, 1.0);
        acc.append_order(Order::limit(Side::Sell, 110.0, 55.0).unwrap());

        // TODO: validate limit order with short position
    }

    #[test]
    fn validate_stop_market_order_0() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        let acc = Account::new(1.0, 1.0);

        // TODO: validate stop market order with a fresh account
    }

    #[test]
    fn validate_stop_market_order_1() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test validator for stop market order on account with open position

        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Buy, 50.0, 100.0);

        // TODO: validate stop market order with long position


        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Sell, 50.0, 100.0);

        // TODO: validate stop market order with short position
    }

    #[test]
    fn validate_stop_market_order_2() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test validator for stop market orders on account with open orders
        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        let mut acc = Account::new(1.0, 1.0);
        acc.append_order(Order::limit(Side::Buy, 90.0, 45.0).unwrap());

        // TODO: validate stop market order with open orders
    }

    #[test]
    fn order_cost_market_no_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test market order cost with a fresh account

        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        let acc = Account::new(1.0, 1.0);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (0.0, 1.0));

        let o = Order::market(Side::Sell, 100.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (0.0, 1.0));
    }

    #[test]
    fn order_cost_market_with_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test order cost with an account with a position
        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        // test with long position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Buy, 100.0, 100.0);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (0.0, 1.0));
        let o = Order::market(Side::Sell, 100.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (1.0, 0.0));
        let o = Order::market(Side::Sell, 200.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (1.0, 1.0));

        // test with short position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Sell, 100.0, 100.0);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (1.0, 0.0));
        let o = Order::market(Side::Sell, 100.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (0.0, 1.0));
        let o = Order::market(Side::Buy, 200.0).unwrap();
        assert_eq!(validator.order_cost_market(&o, &acc), (1.0, 1.0));
    }

    #[test]
    fn order_cost_limit_no_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test market order cost with a fresh account

        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        let acc = Account::new(1.0, 1.0);

        let o = Order::limit(Side::Buy, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Sell, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Buy, 90.0, 90.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Sell, 110.0, 110.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Buy, 90.0, 180.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 2.0));
        let o = Order::limit(Side::Sell, 110.0, 220.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 2.0));
        let o = Order::limit(Side::Buy, 110.0, 55.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.5));
        let o = Order::limit(Side::Sell, 90.0, 45.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.5));
    }

    #[test]
    fn order_cost_limit_with_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // test order cost with an account with a position
        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        // test with long position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Buy, 100.0, 100.0);

        let o = Order::limit(Side::Buy, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Sell, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.0));
        let o = Order::limit(Side::Buy, 90.0, 45.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.5));
        let o = Order::limit(Side::Sell, 110.0, 55.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.0));

        // test with short position
        let mut acc = Account::new(1.0, 1.0);
        acc.change_position(Side::Sell, 100.0, 100.0);

        let o = Order::limit(Side::Buy, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.0));
        let o = Order::limit(Side::Sell, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Buy, 90.0, 45.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.0));
        let o = Order::limit(Side::Sell, 110.0, 55.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.5));
    }

    #[test]
    fn order_cost_limit_w_open_orders() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let mut validator = Validator::new(0.0, 0.0);
        validator.update(100.0, 100.0);

        let mut acc = Account::new(1.0, 1.0);
        acc.append_order(Order::limit(Side::Buy, 100.0, 100.0).unwrap());

        let o = Order::limit(Side::Buy, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 1.0));
        let o = Order::limit(Side::Sell, 100.0, 100.0).unwrap();
        assert_eq!(validator.order_cost_limit(&o, &acc), (0.0, 0.5));

        // TODO: test with short position
    }

    #[test]
    fn order_cost_stop_no_position() {
        // TODO:
    }

    #[test]
    fn order_cost_stop_with_position() {
        // TODO::
    }

    #[test]
    fn order_cost_stop_w_open_orders() {
        // TODO:
    }

    #[test]
    fn order_cost_fee() {
        if let Err(_) = pretty_env_logger::try_init() {}

        // Test Validator for proper fee handling

        let fee_taker: f64 = 0.001;
        let mut validator = Validator::new(0.0, fee_taker);
        validator.update(100.0, 100.0);

        let acc = Account::new(1.0, 1.0);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        assert_eq!(validator.order_cost(&o, &acc), (0.0, 1.001));

        let o = Order::market(Side::Sell, 100.0).unwrap();
        assert_eq!(validator.order_cost(&o, &acc), (0.0, 1.001));
    }
}
