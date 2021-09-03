use crate::{Account, Config, FuturesType, Order, OrderError, OrderType, Side, Validator};

const MAX_NUM_LIMIT_ORDERS: usize = 50;
const MAX_NUM_STOP_ORDERS: usize = 50;

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange {
    config: Config,
    account: Account,
    validator: Validator,
    bid: f64,
    ask: f64,
    next_order_id: u64,
    step: u64, // used for synhcronizing orders
    high: f64,
    low: f64,
}

impl Exchange {
    /// Create a new Exchange with the desired config and whether to use candles as infomation source
    pub fn new(config: Config) -> Exchange {
        assert!(config.leverage > 0.0);
        let account = Account::new(
            config.leverage,
            config.starting_balance,
            config.futures_type,
        );
        let validator = Validator::new(config.fee_maker, config.fee_taker, config.futures_type);
        Exchange {
            config,
            account,
            validator,
            bid: 0.0,
            ask: 0.0,
            next_order_id: 0,
            step: 0,
            high: 0.0,
            low: 0.0,
        }
    }

    /// Return a reference to current exchange config
    #[inline(always)]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Return the bid price
    #[inline(always)]
    pub fn bid(&self) -> f64 {
        self.bid
    }

    /// Return the ask price
    #[inline(always)]
    pub fn ask(&self) -> f64 {
        self.ask
    }

    /// Return a reference to Account
    #[inline(always)]
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }

    /// Update the exchange state with new information
    /// ### Parameters
    /// bid: bid price
    /// ask: ask price
    /// timestamp: timestamp usually in milliseconds
    /// high: highest price over last period, use when feeding in candle info, otherwise set high == ask
    /// low: lowest price over last period, use when feeding in candle info, otherwise set low == bid
    /// ### Returns
    /// executed orders
    /// true if position has been liquidated
    #[must_use]
    pub fn update_state(
        &mut self,
        bid: f64,
        ask: f64,
        timestamp: u64,
        high: f64,
        low: f64,
    ) -> (Vec<Order>, bool) {
        debug_assert!(bid <= ask, "make sure bid <= ask");
        debug_assert!(high >= low, "make sure high >= low");
        debug_assert!(high >= ask, "make sure high >= ask");
        debug_assert!(low <= bid, "make sure low <= bid");

        self.bid = bid;
        self.ask = ask;
        self.high = high;
        self.low = low;

        self.validator.update(bid, ask);

        if self.check_liquidation() {
            self.liquidate();
            return (vec![], true);
        }

        self.check_orders();

        self.account.update((bid + ask) / 2.0, timestamp);

        self.step += 1;

        (self.account.executed_orders(), false)
    }

    /// Check if a liquidation event should occur
    fn check_liquidation(&mut self) -> bool {
        // TODO: check_liquidation
        // TODO: test check_liquidation

        false
    }

    /// Execute a market order
    fn execute_market(&mut self, side: Side, amount: f64) {
        debug!(
            "exchange: execute_market: side: {:?}, amount: {}",
            side, amount
        );

        let price: f64 = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        let mut fee = self.config.fee_taker * amount;
        match self.config.futures_type {
            FuturesType::Linear => fee *= price,
            FuturesType::Inverse => fee /= price,
        }
        self.account.deduce_fees(fee);
        self.account.change_position(side, amount, price);
    }

    /// Execute a limit order, once triggered
    fn execute_limit(&mut self, side: Side, price: f64, amount: f64) {
        // TODO: log_limit_order_fill
        //self.account.acc_tracker_mut().log_limit_order_fill();

        let mut fee = self.config.fee_maker * amount;
        match self.config.futures_type {
            FuturesType::Linear => fee *= price,
            FuturesType::Inverse => fee /= price,
        }
        self.account.deduce_fees(fee);
        self.account.change_position(side, amount, price);
    }

    /// Perform a liquidation of the account
    fn liquidate(&mut self) {
        // TODO: better liquidate
        debug!("liquidating");
        if self.account.position().size() > 0.0 {
            self.execute_market(Side::Sell, self.account.position().size());
        } else {
            self.execute_market(Side::Buy, self.account.position().size().abs());
        }
    }

    /// Check if any active orders have been triggered by the most recent price action
    /// method is called after new external data has been consumed
    fn check_orders(&mut self) {
        for i in 0..self.account.active_limit_orders().len() {
            match self.account.active_limit_orders()[i].order_type {
                OrderType::Limit => self.handle_limit_order(i),
                _ => panic!("there should only be limit orders in active_limit_orders"),
            }
        }
        for i in 0..self.account.active_stop_orders().len() {
            match self.account.active_stop_orders()[i].order_type {
                OrderType::StopMarket => self.handle_stop_market_order(i),
                _ => panic!("there should only be stop market orders in active_stop_orders"),
            }
        }
    }

    /// Handle stop market order trigger and execution
    fn handle_stop_market_order(&mut self, order_idx: usize) {
        // check if stop order has been triggered
        match self.account().active_stop_orders()[order_idx].side {
            Side::Buy => {
                if self.account().active_stop_orders()[order_idx].trigger_price > self.high {
                    return;
                }
            }
            Side::Sell => {
                if self.account().active_stop_orders()[order_idx].trigger_price < self.low {
                    return;
                }
            }
        }
        self.execute_market(
            self.account().active_stop_orders()[order_idx].side,
            self.account().active_stop_orders()[order_idx].size,
        );
        self.account.finalize_stop_order(order_idx);
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_idx: usize) {
        let o: Order = self.account.active_limit_orders()[order_idx];
        match o.side {
            Side::Buy => {
                // use candle information to specify execution
                if self.low <= o.limit_price {
                    self.execute_limit(o.side, o.limit_price, o.size);
                } else {
                    return;
                }
            }
            Side::Sell => {
                // use candle information to specify execution
                if self.high >= o.limit_price {
                    self.execute_limit(o.side, o.limit_price, o.size);
                } else {
                    return;
                }
            }
        }
        self.account.finalize_limit_order(order_idx);
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    #[must_use]
    pub fn submit_order(&mut self, mut order: Order) -> Result<Order, OrderError> {
        match order.order_type {
            OrderType::StopMarket => {
                if self.account().active_limit_orders().len() >= MAX_NUM_LIMIT_ORDERS {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
            _ => {
                if self.account().active_stop_orders().len() >= MAX_NUM_STOP_ORDERS {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
        }

        self.validator.validate(&order, &self.account)?;

        // assign unique order id
        order.id = self.next_order_id;
        self.next_order_id += 1;

        order.timestamp = self.step;

        match order.order_type {
            OrderType::Market => {
                // immediately execute market order
                self.execute_market(order.side, order.size);

                Ok(order)
            }
            _ => {
                self.account.append_order(order);

                Ok(order)
            }
        }
    }
}
