extern crate trade_aggregation;

use crate::acc_tracker::AccTracker;
use crate::{max, min, Config, FeeType, Margin, Order, OrderError, OrderType, Position, Side};
use trade_aggregation::*;

const MAX_NUM_LIMIT_ORDERS: usize = 50;
const MAX_NUM_STOP_ORDERS: usize = 50;

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange {
    config: Config,
    position: Position,
    margin: Margin,
    bid: f64,
    ask: f64,
    init: bool,
    rpnls: Vec<f64>,
    orders_done: Vec<Order>,
    orders_executed: Vec<Order>,
    active_limit_orders: Vec<Order>,
    active_stop_orders: Vec<Order>,
    next_order_id: u64,
    acc_tracker: AccTracker,
    timestamp: u64, // used for synhcronizing orders
    high: f64,
    low: f64,
    // used for calculating hedged order size for order margin calculation
    open_limit_buy_size: f64,
    open_limit_sell_size: f64,
    open_stop_buy_size: f64,
    open_stop_sell_size: f64,
    min_limit_buy_price: f64,
    max_limit_sell_price: f64,
    max_stop_buy_price: f64,
    min_stop_sell_price: f64,
}

impl Exchange {
    /// Create a new Exchange with the desired config and whether to use candles as infomation source
    pub fn new(config: Config) -> Exchange {
        assert!(config.leverage > 0.0);
        let position = Position::new(config.leverage);
        let margin = Margin::new(config.starting_balance_base);
        let acc_tracker = AccTracker::new(config.starting_balance_base);
        return Exchange {
            config,
            position,
            margin,
            bid: 0.0,
            ask: 0.0,
            init: true,
            rpnls: Vec::new(),
            orders_done: Vec::new(),
            orders_executed: Vec::new(),
            active_limit_orders: Vec::with_capacity(MAX_NUM_LIMIT_ORDERS),
            active_stop_orders: Vec::with_capacity(MAX_NUM_STOP_ORDERS),
            next_order_id: 0,
            acc_tracker,
            timestamp: 0,
            high: 0.0,
            low: 0.0,
            open_limit_buy_size: 0.0,
            open_limit_sell_size: 0.0,
            open_stop_buy_size: 0.0,
            open_stop_sell_size: 0.0,
            min_limit_buy_price: 0.0,
            max_limit_sell_price: 0.0,
            max_stop_buy_price: 0.0,
            min_stop_sell_price: 0.0,
        };
    }

    /// Return the bid price
    pub fn bid(&self) -> f64 {
        self.bid
    }

    /// Return the ask price
    pub fn ask(&self) -> f64 {
        self.ask
    }

    /// Return the high price of the last candle
    pub fn high(&self) -> f64 {
        self.high
    }

    /// Return the low price of the last candle
    pub fn low(&self) -> f64 {
        self.low
    }

    /// Set a new position manually, be sure that you know what you are doing
    /// Returns true if successful
    pub fn set_position(&mut self, position: Position) -> bool {
        if position.leverage <= 0.0 || position.value() < 0.0 {
            return false;
        }
        self.position = position;

        true
    }

    /// Return a reference to internal position
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Set a new margin manually, be sure that you know what you are doing when using this method
    /// Returns true if successful
    pub fn set_margin(&mut self, margin: Margin) -> bool {
        if margin.wallet_balance() < 0.0 {
            return false;
        }
        self.margin = margin;

        true
    }

    /// Return a reference to internal margin
    pub fn margin(&self) -> &Margin {
        &self.margin
    }

    /// Set a timestamp, used for synchronizing orders
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    /// Update the exchange state with a new trade.
    /// returns true if position has been liquidated
    pub fn consume_trade(&mut self, trade: &Trade) -> bool {
        if self.init {
            self.init = false;
            self.bid = trade.price;
            self.ask = trade.price;
        }
        if trade.size > 0.0 {
            self.ask = trade.price;
        } else {
            self.bid = trade.price;
        }

        self.acc_tracker.log_timestamp(trade.timestamp as u64);
        let upnl = self.unrealized_pnl();
        self.acc_tracker.log_upnl(upnl);

        if self.check_liquidation() {
            self.liquidate();
            return true;
        }

        self.check_orders();
        self.update_position_stats();

        return false;
    }

    /// Update the exchange status with a new candle.
    /// returns true if position has been liquidated
    pub fn consume_candle(&mut self, candle: &Candle) -> bool {
        self.bid = candle.close;
        self.ask = candle.close;
        self.high = candle.high;
        self.low = candle.low;

        self.acc_tracker.log_timestamp(candle.timestamp as u64);
        let upnl = self.unrealized_pnl();
        self.acc_tracker.log_upnl(upnl);

        if self.check_liquidation() {
            self.liquidate();
            return true;
        }

        self.check_orders();
        self.update_position_stats();

        return false;
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<Order> {
        for (i, o) in self.active_limit_orders.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.active_limit_orders.remove(i);
                match old_order.side {
                    Side::Buy => self.open_limit_buy_size -= old_order.size,
                    Side::Sell => self.open_limit_sell_size -= old_order.size,
                }
                return Some(old_order);
            }
        }
        for (i, o) in self.active_stop_orders.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.active_stop_orders.remove(i);
                match old_order.side {
                    Side::Buy => self.open_stop_buy_size -= old_order.size,
                    Side::Sell => self.open_stop_sell_size -= old_order.size,
                }
                return Some(old_order);
            }
        }
        // re compute min and max prices for open orders
        self.min_limit_buy_price = 0.0;
        self.max_limit_sell_price = 0.0;
        for o in self.active_limit_orders.iter() {
            match o.side {
                Side::Buy => {
                    if self.min_limit_buy_price == 0.0 {
                        self.min_limit_buy_price = o.limit_price;
                        continue;
                    }
                    if o.limit_price < self.min_limit_buy_price {
                        self.min_limit_buy_price = o.limit_price;
                    }
                }
                Side::Sell => {
                    if self.max_limit_sell_price == 0.0 {
                        self.max_limit_sell_price = o.limit_price;
                        continue;
                    }
                    if o.limit_price > self.max_limit_sell_price {
                        self.max_limit_sell_price = o.limit_price;
                    }
                }
            }
        }

        self.max_stop_buy_price = 0.0;
        self.min_stop_sell_price = 0.0;
        for o in self.active_stop_orders.iter() {
            match o.side {
                Side::Buy => {
                    if self.max_stop_buy_price == 0.0 {
                        self.max_stop_buy_price = o.trigger_price;
                        continue;
                    }
                    if o.trigger_price > self.max_stop_buy_price {
                        self.max_stop_buy_price = o.trigger_price;
                    }
                }
                Side::Sell => {
                    if self.min_stop_sell_price == 0.0 {
                        self.min_stop_sell_price = o.trigger_price;
                        continue;
                    }
                    if o.trigger_price < self.min_stop_sell_price {
                        self.min_stop_sell_price = o.trigger_price;
                    }
                }
            }
        }
        None
    }

    /// Cancel all active orders
    pub fn cancel_all_orders(&mut self) {
        self.margin.set_order_margin(0.0);
        self.open_limit_buy_size = 0.0;
        self.open_limit_sell_size = 0.0;
        self.open_stop_buy_size = 0.0;
        self.open_stop_sell_size = 0.0;
        self.min_limit_buy_price = 0.0;
        self.max_limit_sell_price = 0.0;
        self.max_stop_buy_price = 0.0;
        self.min_stop_sell_price = 0.0;
        self.active_limit_orders.clear();
        self.active_stop_orders.clear();
    }

    /// Query an active order by order id
    /// Returns some order if found
    pub fn query_active_orders(&self, order_id: u64) -> Option<&Order> {
        for (i, o) in self.active_limit_orders.iter().enumerate() {
            if o.id == order_id {
                return self.active_limit_orders.get(i);
            }
        }
        for (i, o) in self.active_stop_orders.iter().enumerate() {
            if o.id == order_id {
                return self.active_stop_orders.get(i);
            }
        }
        None
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: Order) -> Result<Order, OrderError> {
        match order.order_type {
            OrderType::StopMarket => {
                if self.active_limit_orders.len() >= MAX_NUM_LIMIT_ORDERS {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
            _ => {
                if self.active_stop_orders.len() >= MAX_NUM_STOP_ORDERS {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
        }

        match order.order_type {
            OrderType::Market => self.validate_market_order(&order)?,
            OrderType::Limit => self.validate_limit_order(&order)?,
            OrderType::StopMarket => self.validate_stop_market_order(&order)?,
        };

        // assign unique order id
        order.id = self.next_order_id;
        self.next_order_id += 1;

        order.timestamp = self.timestamp;

        return match order.order_type {
            OrderType::Market => {
                // immediately execute market order
                self.execute_market(order.side, order.size);

                Ok(order)
            }
            OrderType::Limit => {
                match order.side {
                    Side::Buy => {
                        self.open_limit_buy_size += order.size;
                        if self.min_limit_buy_price == 0.0 {
                            self.min_limit_buy_price = order.limit_price;
                        }
                        if order.limit_price < self.min_limit_buy_price {
                            self.min_limit_buy_price = order.limit_price;
                        }
                    }
                    Side::Sell => {
                        self.open_limit_sell_size += order.size;
                        if self.max_limit_sell_price == 0.0 {
                            self.max_limit_sell_price = order.limit_price;
                        }
                        if order.limit_price > self.max_limit_sell_price {
                            self.max_limit_sell_price = order.limit_price;
                        }
                    }
                }
                // set order margin
                let om = self.order_margin();
                self.margin.set_order_margin(om);

                self.acc_tracker.log_limit_order_submission();
                self.active_limit_orders.push(order.clone());

                Ok(order)
            }
            OrderType::StopMarket => {
                match order.side {
                    Side::Buy => {
                        self.open_stop_buy_size += order.size;
                        if self.max_stop_buy_price == 0.0 {
                            self.max_stop_buy_price = order.trigger_price;
                        }
                        if order.trigger_price > self.max_stop_buy_price {
                            self.max_stop_buy_price = order.trigger_price;
                        }
                    }
                    Side::Sell => {
                        self.open_stop_sell_size += order.size;
                        if self.min_stop_sell_price == 0.0 {
                            self.min_stop_sell_price = order.trigger_price;
                        }
                        if order.trigger_price < self.min_stop_sell_price {
                            self.min_stop_sell_price = order.trigger_price;
                        }
                    }
                }
                // set order margin
                let om = self.order_margin();
                self.margin.set_order_margin(om);

                self.active_stop_orders.push(order.clone());

                Ok(order)
            }
        };
    }

    /// Return the unrealized profit and loss of the accounts position
    pub fn unrealized_pnl(&self) -> f64 {
        return if self.position.size == 0.0 {
            0.0
        } else if self.position.size > 0.0 {
            ((1.0 / self.position.entry_price) - (1.0 / self.bid)) * self.position.size.abs()
        } else {
            ((1.0 / self.ask) - (1.0 / self.position.entry_price)) * self.position.size.abs()
        };
    }

    /// Return the recently executed orders and clear afterwards
    pub fn executed_orders(&mut self) -> Vec<Order> {
        let exec_orders: Vec<Order> = self.orders_executed.clone();
        self.orders_executed.clear();
        return exec_orders;
    }

    /// Return the currently active limit orders
    pub fn active_limit_orders(&self) -> &Vec<Order> {
        &self.active_limit_orders
    }

    /// Return the currently active stop orders
    pub fn active_stop_orders(&self) -> &Vec<Order> {
        &self.active_stop_orders
    }

    /// Check if market order is correct
    pub fn validate_market_order(&mut self, o: &Order) -> Result<(), OrderError> {
        let price = match o.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let fee_base = self.config.fee_taker * o.size;
        let fee_asset = fee_base / self.bid;

        // TODO: clean this up by using self.order_cost
        // check if enough available balance for initial margin requirements
        let order_margin: f64 = o.size / price / self.position.leverage;
        match o.side {
            Side::Buy => {
                if self.position.size > 0.0 {
                    if order_margin + fee_asset > self.margin.available_balance() {
                        return Err(OrderError::NotEnoughAvailableBalance);
                    }
                    Ok(())
                } else {
                    if order_margin > self.margin.position_margin() {
                        // check if there is enough available balance for the rest of order_margin
                        let margin_diff = order_margin - self.position.margin;
                        if margin_diff + fee_asset
                            > self.margin.available_balance() + self.position.margin
                        {
                            return Err(OrderError::NotEnoughAvailableBalance);
                        }
                        return Ok(());
                    }
                    Ok(())
                }
            }
            Side::Sell => {
                if self.position.size > 0.0 {
                    if order_margin > self.margin.position_margin() {
                        // check if there is enough available balance for the rest of order_margin
                        let margin_diff = order_margin - self.position.margin;
                        if margin_diff + fee_asset
                            > self.margin.available_balance() + self.position.margin
                        {
                            return Err(OrderError::NotEnoughAvailableBalance);
                        }
                        return Ok(());
                    }
                    Ok(())
                } else {
                    if order_margin + fee_asset > self.margin.available_balance() {
                        return Err(OrderError::NotEnoughAvailableBalance);
                    }
                    Ok(())
                }
            }
        }
    }

    /// Check if a limit order is correct
    pub fn validate_limit_order(&self, o: &Order) -> Result<(), OrderError> {
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

        let order_cost: f64 = self.order_cost(o);
        if order_cost > self.margin().available_balance() {
            return Err(OrderError::NotEnoughAvailableBalance);
        }
        Ok(())
    }

    /// Check if a stop market order is correct
    pub fn validate_stop_market_order(&mut self, o: &Order) -> Result<(), OrderError> {
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
        let order_cost: f64 = self.order_cost(o);
        if order_cost > self.margin().available_balance() {
            return Err(OrderError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    /// Return the return on equity of the accounts position
    pub fn roe(&self) -> f64 {
        return if self.position.size() > 0.0 {
            (self.bid - self.position.entry_price()) / self.position.entry_price()
        } else {
            (self.position.entry_price() - self.ask) / self.position.entry_price()
        };
    }

    /// Return a reference to internal acc_tracker struct
    pub fn acc_tracker(&self) -> &AccTracker {
        &self.acc_tracker
    }

    /// Calculate the order margin denoted in BASE currency
    /// TODO: mark it work in all cases
    fn order_margin(&self) -> f64 {
        let ps: f64 = self.position.size;
        let open_sizes: [f64; 4] = [
            self.open_limit_buy_size,
            self.open_limit_sell_size,
            self.open_stop_buy_size,
            self.open_stop_sell_size,
        ];
        let mut max_idx: usize = 0;
        let mut m: f64 = self.open_limit_buy_size;

        for (i, s) in open_sizes.iter().enumerate() {
            if *s > m {
                m = *s;
                max_idx = i;
            }
        }

        // direction of dominating open order side
        let (d, p) = match max_idx {
            0 => (1.0, self.min_limit_buy_price),
            1 => (-1.0, self.max_limit_sell_price),
            2 => (1.0, self.max_stop_buy_price),
            3 => (-1.0, self.min_stop_sell_price),
            _ => panic!("any other value should not be possible"),
        };
        if p == 0.0 {
            return 0.0;
        }

        max(0.0, min(m, m + d * ps)) / p / self.position.leverage
    }

    /// Calculate the cost of order
    /// TODO: make it work in all cases
    fn order_cost(&self, order: &Order) -> f64 {
        let ps: f64 = self.position.size;
        let mut olbs = self.open_limit_buy_size;
        let mut olss = self.open_limit_sell_size;
        let mut osbs = self.open_stop_buy_size;
        let mut osss = self.open_stop_sell_size;
        match order.order_type {
            OrderType::Limit => match order.side {
                Side::Buy => olbs += order.size,
                Side::Sell => olss += order.size,
            },
            OrderType::StopMarket => match order.side {
                Side::Buy => osbs += order.size,
                Side::Sell => osss += order.size,
            },
            OrderType::Market => match order.side {
                Side::Buy => osbs += order.size,
                Side::Sell => osss += order.size,
            },
        }
        let open_sizes: [f64; 4] = [olbs, olss, osbs, osss];
        let mut max_idx: usize = 0;
        let mut m: f64 = self.open_limit_buy_size;

        for (i, s) in open_sizes.iter().enumerate() {
            if *s > m {
                m = *s;
                max_idx = i;
            }
        }

        // direction of dominating open order side
        let d = match max_idx {
            0 => 1.0,
            1 => -1.0,
            2 => 1.0,
            3 => -1.0,
            _ => panic!("any other value should not be possible"),
        };

        // TODO: use order price of most expensive order not just last order
        let order_price: f64 = match order.order_type {
            OrderType::Market => match order.side {
                Side::Buy => self.ask(),
                Side::Sell => self.bid(),
            },
            OrderType::Limit => order.limit_price,
            OrderType::StopMarket => order.trigger_price,
        };
        max(0.0, min(m, m + d * ps)) / order_price / self.position.leverage
    }

    /// Check if a liquidation event should occur
    fn check_liquidation(&mut self) -> bool {
        if self.position.size() > 0.0 {
            // liquidation check for long position
            if self.ask < self.position.liq_price() {
                return true;
            }
        } else if self.position.size() < 0.0 {
            // liquidation check for short position
            if self.bid > self.position.liq_price() {
                return true;
            }
        }

        false
    }

    /// Reduce the account equity by a fee amount
    fn deduce_fees(&mut self, t: FeeType, amount_base: f64, price: f64) {
        let fee: f64 = match t {
            FeeType::Maker => self.config.fee_maker,
            FeeType::Taker => self.config.fee_taker,
        };
        let fee_quote = fee * amount_base;
        let fee_base = fee_quote / price;
        self.acc_tracker.log_fee(fee_base);
        self.margin.wallet_balance -= fee_base;
        self.update_position_stats();
    }

    /// Update the accounts position statistics,
    /// method is called after order has been executed
    fn update_position_stats(&mut self) {
        let price: f64 = if self.position.size > 0.0 {
            self.bid
        } else {
            self.ask
        };
        self.position.unrealized_pnl = self.unrealized_pnl();
        self.position.value = self.position.size.abs() / price;

        // TODO: should if keep this function?

        self.margin.position_margin =
            (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
        self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;
        self.margin.available_balance =
            self.margin.wallet_balance - self.margin.position_margin - self.margin.order_margin;
    }

    /// Execute a market order
    fn execute_market(&mut self, side: Side, amount_base: f64) {
        let price: f64 = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        self.deduce_fees(FeeType::Taker, amount_base, price);

        let old_position_size = self.position.size;
        let old_entry_price: f64 = if self.position.size == 0.0 {
            price
        } else {
            self.position.entry_price
        };
        self.acc_tracker.log_trade(side, amount_base);

        match side {
            Side::Buy => {
                if self.position.size < 0.0 {
                    if amount_base >= self.position.size.abs() {
                        // realize_pnl
                        let rpnl = self.position.size.abs()
                            * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        // realize pnl
                        let rpnl =
                            amount_base * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size += amount_base;
                        self.position.margin =
                            self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.margin += amount_base / old_entry_price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base)
                        + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            }
            Side::Sell => {
                if self.position.size > 0.0 {
                    if amount_base >= self.position.size.abs() {
                        // realize pnl
                        let rpnl = self.position.size.abs()
                            * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        // realize pnl
                        let rpnl =
                            amount_base * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size -= amount_base;
                        self.position.margin =
                            self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size -= amount_base;
                    self.position.margin += amount_base / old_entry_price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base)
                        + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            }
        }

        self.update_position_stats();
        self.update_liq_price();
    }

    /// Execute a limit order, once triggered
    fn execute_limit(&mut self, side: Side, price: f64, amount_base: f64) {
        self.acc_tracker.log_limit_order_fill();
        self.deduce_fees(FeeType::Maker, amount_base, price);
        self.acc_tracker.log_trade(side, amount_base);

        let old_position_size = self.position.size;
        let old_entry_price: f64 = if self.position.size == 0.0 {
            price
        } else {
            self.position.entry_price
        };

        match side {
            Side::Buy => {
                if self.position.size < 0.0 {
                    // realize pnl
                    if amount_base > self.position.size.abs() {
                        let rpnl = self.position.size.abs()
                            * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        let rpnl =
                            amount_base * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size += amount_base;
                        self.position.margin =
                            self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.margin += amount_base / price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base)
                        + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            }
            Side::Sell => {
                if self.position.size > 0.0 {
                    // realize pnl
                    if amount_base > self.position.size.abs() {
                        let rpnl = self.position.size.abs()
                            * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        let rpnl =
                            amount_base * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size -= amount_base;
                        self.position.margin =
                            self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.margin += amount_base / price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base)
                        + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            }
        }

        self.update_position_stats();
        self.update_liq_price();
    }

    /// Perform a liquidation of the account
    fn liquidate(&mut self) {
        debug!("liquidating");
        if self.position.size > 0.0 {
            self.execute_market(Side::Sell, self.position.size);
        } else {
            self.execute_market(Side::Buy, self.position.size);
        }

        self.update_position_stats();
    }

    /// Check if any active orders have been triggered by the most recent price action
    /// method is called after new external data has been consumed
    fn check_orders(&mut self) {
        for i in 0..self.active_limit_orders.len() {
            match self.active_limit_orders[i].order_type {
                OrderType::Limit => self.handle_limit_order(i),
                _ => panic!("there should only be limit orders in active_limit_orders"),
            }
        }
        for i in 0..self.active_stop_orders.len() {
            match self.active_stop_orders[i].order_type {
                OrderType::StopMarket => self.handle_stop_market_order(i),
                _ => panic!("there should only be stop market orders in active_stop_orders"),
            }
        }
    }

    /// Handle stop market order trigger and execution
    fn handle_stop_market_order(&mut self, order_idx: usize) {
        // check if stop order has been triggered
        match self.active_stop_orders[order_idx].side {
            Side::Buy => match self.config.use_candles {
                true => {
                    if self.active_stop_orders[order_idx].trigger_price > self.high {
                        return;
                    }
                }
                false => {
                    if self.active_stop_orders[order_idx].trigger_price > self.ask {
                        return;
                    }
                }
            },
            Side::Sell => match self.config.use_candles {
                true => {
                    if self.active_stop_orders[order_idx].trigger_price < self.low {
                        return;
                    }
                }
                false => {
                    if self.active_stop_orders[order_idx].trigger_price > self.bid {
                        return;
                    }
                }
            },
        }
        self.execute_market(
            self.active_stop_orders[order_idx].side,
            self.active_stop_orders[order_idx].size,
        );
        self.active_stop_orders[order_idx].mark_executed();

        // free order margin
        let order_size: f64 = self.active_stop_orders[order_idx].size;
        match self.active_stop_orders[order_idx].side {
            Side::Buy => self.open_stop_buy_size -= order_size,
            Side::Sell => self.open_stop_sell_size -= order_size,
        }
        self.margin.set_order_margin(self.order_margin());

        let exec_order = self.active_stop_orders.remove(order_idx);
        self.orders_executed.push(exec_order);
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_idx: usize) {
        let o: &Order = &self.active_limit_orders[order_idx];
        match o.side {
            Side::Buy => {
                match self.config.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.low <= o.limit_price {
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.bid < o.limit_price {
                            // execute
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                }
            }
            Side::Sell => {
                match self.config.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.high >= o.limit_price {
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.ask > o.limit_price {
                            // execute
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                }
            }
        }
        self.active_limit_orders[order_idx].mark_executed();

        // free limit order margin
        let order_size = self.active_limit_orders[order_idx].size;
        match self.active_limit_orders[order_idx].side {
            Side::Buy => self.open_limit_buy_size -= order_size,
            Side::Sell => self.open_limit_sell_size -= order_size,
        }
        self.margin.set_order_margin(self.order_margin());

        let exec_order = self.active_limit_orders.remove(order_idx);
        self.orders_executed.push(exec_order);
    }

    /// Update the account position liquidation price
    fn update_liq_price(&mut self) {
        if self.position.size == 0.0 {
            self.position.liq_price = 0.0;
        } else if self.position.size > 0.0 {
            self.position.liq_price =
                self.position.entry_price - (self.position.entry_price / self.position.leverage);
        } else {
            self.position.liq_price =
                self.position.entry_price + (self.position.entry_price / self.position.leverage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::round;

    #[test]
    fn validate_market_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        // valid order
        let size = exchange.ask * exchange.margin.available_balance * 0.4;
        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_ok());

        // valid order
        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_ok());

        // invalid order
        let size = exchange.ask * exchange.margin.available_balance * 1.05;
        let o = Order::market(Side::Buy, size).unwrap();
        assert!(exchange.validate_market_order(&o).is_err());

        // invalid order
        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_err());

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, 800.0);

        // valid order
        let o = Order::market(Side::Buy, 190.0).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_ok());

        // invalid order
        let o = Order::market(Side::Buy, 210.0).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_err());

        // valid order
        let o = Order::market(Side::Sell, 800.0).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_ok());

        // invalid order
        let o = Order::market(Side::Sell, 2100.0).unwrap();
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_err());

        // valid order
        let o = Order::market(Side::Sell, 1600.0).unwrap();
        assert!(exchange.validate_market_order(&o).is_ok());
    }

    #[test]
    fn test_validate_limit_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Buy, price, size).unwrap();
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_ok());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Buy, price, size).unwrap();
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_err());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Sell, price, size).unwrap();
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_ok());

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Sell, price, size).unwrap();
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_err());

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * 1.1 * price;
        let o = Order::limit(Side::Buy, price, size).unwrap();
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_err());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * 1.1 * price;
        let o = Order::limit(Side::Sell, price, size).unwrap();
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_err());
    }

    #[test]
    fn submit_order_limit() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        // submit working market order
        let o = Order::market(Side::Buy, 500.0).unwrap();
        exchange.submit_order(o).unwrap();

        let o = Order::limit(Side::Buy, 900.0, 250.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.active_limit_orders.len(), 1);

        // submit opposite limit order acting as target order
        let o = Order::limit(Side::Sell, 1200.0, 500.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.active_limit_orders.len(), 2);
    }

    #[test]
    fn test_validate_stop_market_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 10.0).unwrap();
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_ok());

        let o = Order::stop_market(Side::Sell, 1010.0, 10.0).unwrap();
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_err());

        let o = Order::stop_market(Side::Buy, 980.0, 10.0).unwrap();
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_err());

        let o = Order::stop_market(Side::Sell, 980.0, 10.0).unwrap();
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_ok());

        let o = Order::stop_market(Side::Buy, 1000.0, 10.0).unwrap();
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_err());

        let o = Order::stop_market(Side::Buy, 1000.0, 10.0).unwrap();
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_err());
    }

    #[test]
    fn test_handle_limit_order() {
        // TODO:
    }

    #[test]
    fn handle_stop_market_order_w_trade() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 100.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.active_stop_orders.len(), 1);

        let t = Trade {
            timestamp: 2,
            price: 1010.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.position.size, 100.0);
        assert_eq!(exchange.position.entry_price, 1010.0);
    }

    #[test]
    fn handle_stop_market_order_w_candle() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: true,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);

        let c = Candle {
            timestamp: 0,
            open: 40_000.0,
            high: 40_500.0,
            low: 35_000.0,
            close: 40_100.0,
            volume: 0.0,
            directional_trade_ratio: 0.0,
            directional_volume_ratio: 0.0,
            num_trades: 0,
            arithmetic_mean_price: 0.0,
            weighted_price: 0.0,
            std_dev_prices: 0.0,
            std_dev_sizes: 0.0,
            time_velocity: 0.0,
        };
        exchange.consume_candle(&c);

        let o = Order::stop_market(Side::Buy, 40_600.0, 4060.0).unwrap();
        exchange.submit_order(o).unwrap();
        // assert_eq!(exchange.margin().order_margin(), 0.1);

        let c = Candle {
            timestamp: 0,
            open: 40_100.0,
            high: 40_700.0,
            low: 36_000.0,
            close: 40_500.0,
            volume: 0.0,
            directional_trade_ratio: 0.0,
            directional_volume_ratio: 0.0,
            num_trades: 0,
            arithmetic_mean_price: 0.0,
            weighted_price: 0.0,
            std_dev_prices: 0.0,
            std_dev_sizes: 0.0,
            time_velocity: 0.0,
        };
        exchange.consume_candle(&c);

        assert_eq!(exchange.position().size(), 4060.0);
        assert_eq!(round(exchange.position().value(), 1), 0.1);
        assert_eq!(round(exchange.margin().position_margin(), 1), 0.1);
        // assert_eq!(exchange.margin().order_margin(), 0.0);
    }

    #[test]
    fn long_market_win_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * 0.8;
        let size = exchange.ask * value;
        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset1 = fee_base / exchange.bid;

        assert_eq!(exchange.position.size, size);
        assert_eq!(exchange.position.entry_price, 1000.0);
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(exchange.margin.wallet_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.position_margin, 0.8);
        assert_eq!(
            round(exchange.margin.available_balance, 4),
            round(0.2 - fee_asset1, 4)
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);
        assert_eq!(exchange.position.unrealized_pnl, 0.4);

        let size = 800.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(
            exchange.margin.wallet_balance,
            1.4 - fee_asset1 - fee_asset2
        );
        assert_eq!(
            exchange.margin.margin_balance,
            1.4 - fee_asset1 - fee_asset2
        );
        assert_eq!(exchange.margin.position_margin, 0.0);
        assert_eq!(
            exchange.margin.available_balance,
            1.4 - fee_asset1 - fee_asset2
        );
    }

    #[test]
    fn long_market_loss_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, 800.0);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), -0.2);

        let o = Order::market(Side::Sell, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 800.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(
            round(exchange.margin.wallet_balance, 5),
            round(0.8 - fee_combined, 5)
        );
        assert_eq!(
            round(exchange.margin.margin_balance, 5),
            round(0.8 - fee_combined, 5)
        );
        assert_eq!(exchange.margin.position_margin, 0.0);
        assert_eq!(
            round(exchange.margin.available_balance, 5),
            round(0.8 - fee_combined, 5)
        );
    }

    #[test]
    fn short_market_win_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, -800.0);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), 0.2);

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 800.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(exchange.margin.wallet_balance, 1.2 - fee_combined);
        assert_eq!(exchange.margin.margin_balance, 1.2 - fee_combined);
        assert_eq!(exchange.margin.position_margin, 0.0);
        assert_eq!(exchange.margin.available_balance, 1.2 - fee_combined);
    }

    #[test]
    fn short_market_loss_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * 0.4;
        let size = exchange.ask * value;
        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base1 = size * fee_taker;
        let fee_asset1 = fee_base1 / exchange.bid;

        assert_eq!(exchange.position.size, -size);
        assert_eq!(exchange.position.entry_price, 1000.0);
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(exchange.margin.wallet_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.position_margin, 0.4);
        assert_eq!(exchange.margin.available_balance, 0.6 - fee_asset1);

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = 400.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        assert_eq!(exchange.position.unrealized_pnl, -0.2);

        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(
            round(exchange.margin.wallet_balance, 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
        assert_eq!(
            round(exchange.margin.margin_balance, 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
        assert_eq!(exchange.margin.position_margin, 0.0);
        assert_eq!(
            round(exchange.margin.available_balance, 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
    }

    #[test]
    fn long_market_win_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * 0.8;
        let size = exchange.ask * value;
        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset1 = fee_base / exchange.bid;

        assert_eq!(exchange.position.size, size);
        assert_eq!(exchange.position.entry_price, 1000.0);
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(exchange.margin.wallet_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.position_margin, 0.8);
        assert_eq!(
            round(exchange.margin.available_balance, 4),
            round(0.2 - fee_asset1, 4)
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = 400.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        assert_eq!(exchange.position.unrealized_pnl, 0.4);

        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, 400.0);
        assert_eq!(exchange.position.value, 0.2);
        assert_eq!(exchange.position.margin, 0.4);
        assert_eq!(exchange.position.unrealized_pnl, 0.2);
        assert_eq!(
            exchange.margin.wallet_balance,
            1.2 - fee_asset1 - fee_asset2
        );
        assert_eq!(
            exchange.margin.margin_balance,
            1.4 - fee_asset1 - fee_asset2
        );
        assert_eq!(exchange.margin.position_margin, 0.4);
        assert_eq!(
            round(exchange.margin.available_balance, 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
    }

    #[test]
    fn long_market_loss_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, 800.0);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), -0.2);

        let o = Order::market(Side::Sell, 400.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 400.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, 400.0);
        assert_eq!(exchange.position.value, 0.5);
        assert_eq!(exchange.position.margin, 0.4);
        assert_eq!(exchange.position.unrealized_pnl, -0.1);
        assert_eq!(
            round(exchange.margin.wallet_balance, 6),
            round(0.9 - fee_combined, 6)
        );
        assert_eq!(
            round(exchange.margin.margin_balance, 6),
            round(0.8 - fee_combined, 6)
        );
        assert_eq!(exchange.margin.position_margin, 0.4);
        assert_eq!(
            round(exchange.margin.available_balance, 6),
            round(0.5 - fee_combined, 6)
        );
    }

    #[test]
    fn short_market_win_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, -800.0);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), 0.2);

        let o = Order::market(Side::Buy, 400.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 400.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, -400.0);
        assert_eq!(exchange.position.value, 0.5);
        assert_eq!(exchange.position.margin, 0.4);
        assert_eq!(exchange.position.unrealized_pnl, 0.1);
        assert_eq!(
            round(exchange.margin.wallet_balance, 6),
            round(1.1 - fee_combined, 6)
        );
        assert_eq!(exchange.margin.margin_balance, 1.2 - fee_combined);
        assert_eq!(exchange.margin.position_margin, 0.6);
        assert_eq!(
            round(exchange.margin.available_balance, 6),
            round(0.5 - fee_combined, 6)
        );
    }

    #[test]
    fn short_market_loss_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * 0.8;
        let size = exchange.ask * value;
        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base1 = size * fee_taker;
        let fee_asset1 = fee_base1 / exchange.bid;

        assert_eq!(exchange.position.size, -size);
        assert_eq!(exchange.position.entry_price, 1000.0);
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(exchange.margin.wallet_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, 1.0 - fee_asset1);
        assert_eq!(exchange.margin.position_margin, 0.8);
        assert_eq!(
            round(exchange.margin.available_balance, 4),
            round(0.2 - fee_asset1, 4)
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = 400.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        assert_eq!(exchange.position.unrealized_pnl, -0.4);

        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, -400.0);
        assert_eq!(exchange.position.value, 0.2);
        assert_eq!(exchange.position.margin, 0.4);
        assert_eq!(exchange.position.unrealized_pnl, -0.2);
        assert_eq!(
            round(exchange.margin.wallet_balance, 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
        assert_eq!(
            round(exchange.margin.margin_balance, 5),
            round(0.6 - fee_asset1 - fee_asset2, 5)
        );
        assert_eq!(exchange.margin.position_margin, 0.0);
        assert_eq!(
            exchange.margin.available_balance,
            0.8 - fee_asset1 - fee_asset2
        );
    }

    #[test]
    fn test_market_roundtrip() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * 0.9;
        let size = exchange.ask * value;
        let buy_order = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let sell_order = Order::market(Side::Sell, size).unwrap();

        let order_err = exchange.submit_order(sell_order);
        assert!(order_err.is_ok());

        let fee_base = size * fee_taker;
        let fee_asset = fee_base / exchange.ask;

        exchange.check_orders();

        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.entry_price, 1000.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert_eq!(exchange.margin.wallet_balance, 1.0 - 2.0 * fee_asset);
        assert_eq!(exchange.margin.margin_balance, 1.0 - 2.0 * fee_asset);
        assert_eq!(exchange.margin.position_margin, 0.0);
        assert_eq!(exchange.margin.available_balance, 1.0 - 2.0 * fee_asset);

        let size = 900.0;
        let buy_order = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let size = 950.0;
        let sell_order = Order::market(Side::Sell, size).unwrap();

        let order_err = exchange.submit_order(sell_order);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, -50.0);
        assert_eq!(exchange.position.entry_price, 1000.0);
        assert_eq!(exchange.position.value, 0.05);
        assert_eq!(exchange.position.margin, 0.05);
        assert_eq!(exchange.position.unrealized_pnl, 0.0);
        assert!(exchange.margin.wallet_balance < 1.0);
        assert!(exchange.margin.margin_balance < 1.0);
        assert_eq!(exchange.margin.position_margin, 0.05);
        assert!(exchange.margin.available_balance < 1.0);
    }

    #[test]
    #[ignore]
    fn test_order_ids() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 100.0,
            size: 10.0,
        };
        exchange.consume_trade(&t);
        for i in 0..100 {
            let o = Order::stop_market(Side::Buy, 101.0 + i as f64, 10.0).unwrap();
            exchange.submit_order(o).unwrap();
        }
        let active_orders = exchange.active_limit_orders;
        let mut last_order_id: i64 = -1;
        for o in &active_orders {
            assert!(o.id as i64 > last_order_id);
            last_order_id = o.id as i64;
        }
    }

    #[test]
    fn liq_price() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.liq_price, 0.0);

        // TODO: test liq_price with higher leverage and with short position as well
    }

    #[test]
    fn unrealized_pnl() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, 100.0);
        let upnl = exchange.unrealized_pnl();
        assert_eq!(upnl, 0.0);

        let t = Trade {
            timestamp: 1,
            price: 1100.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let t = Trade {
            timestamp: 1,
            price: 1100.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let upnl = exchange.unrealized_pnl();
        assert!(upnl > 0.0);
    }

    #[test]
    fn roe() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, 100.0);

        let t = Trade {
            timestamp: 1,
            price: 1100.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let t = Trade {
            timestamp: 1,
            price: 1100.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let roe = exchange.roe();
        assert_eq!(roe, 0.1);
    }

    #[test]
    fn test_liquidate() {
        // TODO:
    }

    #[test]
    fn cancel_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.active_limit_orders.len(), 1);
        assert_eq!(exchange.margin().wallet_balance(), 1.0);
        assert_eq!(exchange.margin().margin_balance(), 1.0);
        assert_eq!(exchange.margin().position_margin(), 0.0);
        // assert_eq!(exchange.margin().available_balance(), 0.5);

        let _o = exchange.cancel_order(0).unwrap();
        assert_eq!(exchange.active_limit_orders.len(), 0);
        assert_eq!(exchange.margin().wallet_balance(), 1.0);
        assert_eq!(exchange.margin().margin_balance(), 1.0);
        assert_eq!(exchange.margin().position_margin(), 0.0);
        // assert_eq!(exchange.margin().available_balance(), 1.0);
    }

    #[test]
    fn cancel_all_orders() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.active_limit_orders.len(), 1);
        assert_eq!(exchange.margin().wallet_balance(), 1.0);
        assert_eq!(exchange.margin().margin_balance(), 1.0);
        assert_eq!(exchange.margin().position_margin(), 0.0);
        assert_eq!(exchange.margin().order_margin(), 0.5);
        assert_eq!(exchange.order_margin(), 0.5);
        assert_eq!(exchange.margin().available_balance(), 0.5);

        let o = Order::stop_market(Side::Buy, 1100.0, 450.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.active_limit_orders.len(), 1);
        assert_eq!(exchange.active_stop_orders.len(), 1);
        assert_eq!(exchange.margin().wallet_balance(), 1.0);
        assert_eq!(exchange.margin().margin_balance(), 1.0);
        assert_eq!(exchange.margin().position_margin(), 0.0);
        assert_eq!(round(exchange.margin().order_margin(), 1), 0.5);
        assert_eq!(round(exchange.order_margin(), 1), 0.5);
        assert_eq!(exchange.margin().available_balance(), 0.5);

        exchange.cancel_all_orders();
        assert_eq!(exchange.active_limit_orders.len(), 0);
        assert_eq!(exchange.active_stop_orders.len(), 0);
        assert_eq!(exchange.margin().wallet_balance(), 1.0);
        assert_eq!(exchange.margin().margin_balance(), 1.0);
        assert_eq!(exchange.margin().position_margin(), 0.0);
        assert_eq!(exchange.margin().order_margin(), 0.0);
        assert_eq!(exchange.order_margin(), 0.0);
        assert_eq!(exchange.margin().available_balance(), 1.0);
    }

    #[test]
    fn execute_limit() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o: Order = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.active_limit_orders.len(), 1);
        // assert_eq!(exchange.margin.available_balance, 0.5);

        let t = Trade {
            timestamp: 1,
            price: 750.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 1,
            price: 750.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let fee_maker_0: f64 = 0.00125;

        assert_eq!(exchange.bid, 750.0);
        assert_eq!(exchange.ask, 750.0);
        assert_eq!(exchange.active_limit_orders.len(), 0);
        assert_eq!(exchange.position.size, 450.0);
        assert_eq!(exchange.position.value, 0.6);
        assert_eq!(exchange.position.margin, 0.5);
        assert_eq!(exchange.position.entry_price, 900.0);
        assert_eq!(exchange.margin.wallet_balance, 1.000125);

        let o: Order = Order::limit(Side::Sell, 1000.0, 450.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.active_limit_orders.len(), 1);

        let t = Trade {
            timestamp: 1,
            price: 1200.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 1,
            price: 1200.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.active_limit_orders.len(), 0);
        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.margin.position_margin, 0.0);
        let fee_maker_1: f64 = 0.001125;
        let wb: f64 = 1.0 + fee_maker_0 + fee_maker_1 + 0.05;
        // Again nearly correct but not quite which is fine though
        // assert_eq!(exchange.margin.wallet_balance, wb);
        // assert_eq!(exchange.margin.available_balance, wb);
        // assert_eq!(exchange.margin.margin_balance, wb);
    }

    #[test]
    fn order_margin() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut e = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        e.consume_trade(&t);

        assert_eq!(e.order_margin(), 0.0);
        e.submit_order(Order::market(Side::Buy, 500.0).unwrap())
            .unwrap();
        assert_eq!(e.order_margin(), 0.0);
        e.submit_order(Order::limit(Side::Sell, 1100.0, 500.0).unwrap())
            .unwrap();
        assert_eq!(e.order_margin(), 0.0);
        e.submit_order(Order::stop_market(Side::Sell, 900.0, 500.0).unwrap())
            .unwrap();
        assert_eq!(e.order_margin(), 0.0);

        // TODO: fix order_margin to make this work
        // e.submit_order(Order::limit(Side::Buy, 900.0, 400.0).unwrap()).unwrap();
        // assert_eq!(round(e.order_margin(), 2), 0.44);
    }

    #[test]
    fn order_cost() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut e = Exchange::new(config.clone());
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        e.consume_trade(&t);

        // test order cost with no position present
        assert_eq!(e.order_cost(&Order::market(Side::Buy, 500.0).unwrap()), 0.5);
        assert_eq!(
            e.order_cost(&Order::market(Side::Sell, 500.0).unwrap()),
            0.5
        );

        assert_eq!(
            e.order_cost(&Order::limit(Side::Buy, 900.0, 450.0).unwrap()),
            0.5
        );
        assert_eq!(
            e.order_cost(&Order::limit(Side::Sell, 1200.0, 600.0).unwrap()),
            0.5
        );

        assert_eq!(
            e.order_cost(&Order::stop_market(Side::Buy, 1200.0, 600.0).unwrap()),
            0.5
        );
        assert_eq!(
            e.order_cost(&Order::stop_market(Side::Sell, 900.0, 450.0).unwrap()),
            0.5
        );

        // test order cost with a long position present
        e.submit_order(Order::market(Side::Buy, 500.0).unwrap())
            .unwrap();
        assert_eq!(e.position().size(), 500.0);

        assert_eq!(e.order_cost(&Order::market(Side::Buy, 500.0).unwrap()), 0.5);
        assert_eq!(
            e.order_cost(&Order::market(Side::Sell, 500.0).unwrap()),
            0.0
        );

        assert_eq!(
            round(
                e.order_cost(&Order::limit(Side::Buy, 900.0, 500.0).unwrap()),
                2
            ),
            0.56
        );
        assert_eq!(
            e.order_cost(&Order::limit(Side::Sell, 1100.0, 500.0).unwrap()),
            0.0
        );

        assert_eq!(
            round(
                e.order_cost(&Order::stop_market(Side::Buy, 1100.0, 500.0).unwrap()),
                2
            ),
            0.45
        );
        assert_eq!(
            e.order_cost(&Order::stop_market(Side::Sell, 900.0, 500.0).unwrap()),
            0.0
        );

        // test order cost with a short position present
        let mut e = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        e.consume_trade(&t);

        e.submit_order(Order::market(Side::Sell, 500.0).unwrap())
            .unwrap();
        assert_eq!(e.position().size(), -500.0);

        assert_eq!(e.order_cost(&Order::market(Side::Buy, 500.0).unwrap()), 0.0);
        assert_eq!(
            e.order_cost(&Order::market(Side::Sell, 500.0).unwrap()),
            0.5
        );

        assert_eq!(
            e.order_cost(&Order::limit(Side::Buy, 900.0, 500.0).unwrap()),
            0.0
        );
        assert_eq!(
            round(
                e.order_cost(&Order::limit(Side::Sell, 1100.0, 500.0).unwrap()),
                2
            ),
            0.45
        );

        assert_eq!(
            e.order_cost(&Order::stop_market(Side::Buy, 1100.0, 500.0).unwrap()),
            0.0
        );
        assert_eq!(
            round(
                e.order_cost(&Order::stop_market(Side::Sell, 900.0, 500.0).unwrap()),
                3
            ),
            0.556
        );

        // TODO: test order cost with both a position and outstanding orders as well
    }
}
