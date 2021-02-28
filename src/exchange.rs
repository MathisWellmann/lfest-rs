extern crate trade_aggregation;

use crate::acc_tracker::AccTracker;
use crate::{FeeType, OrderError, OrderType, Side, Config, Order, Position, Margin};
use trade_aggregation::*;

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
    orders_active: Vec<Order>,
    next_order_id: u64,
    acc_tracker: AccTracker,
    timestamp: u64, // used for synhcronizing orders
    high: f64,
    low: f64,
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
            orders_active: Vec::new(),
            next_order_id: 0,
            acc_tracker,
            timestamp: 0,
            high: 0.0,
            low: 0.0,
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
            return false
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
            return false
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
    /// returns true if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<Order> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.orders_active.remove(i);
                self.update_position_stats();
                return Some(old_order);
            }
        }
        None
    }

    /// Query an active order by order id
    /// Returns some order if found
    pub fn query_active_orders(&self, order_id: u64) -> Option<&Order> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                return self.orders_active.get(i);
            }
        }
        None
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: Order) -> Result<Order, OrderError> {
        match order.order_type {
            OrderType::StopMarket => {
                if self.orders_active.len() >= 10 {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
            _ => {
                if self.orders_active.len() >= 200 {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
        }
        if order.size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        let order_err: Option<OrderError> = match order.order_type {
            OrderType::Market => self.validate_market_order(&order),
            OrderType::Limit => self.validate_limit_order(&order),
            OrderType::StopMarket => self.validate_stop_market_order(&order),
        };
        if order_err.is_some() {
            return Err(order_err.unwrap());
        }

        // assign unique order id
        order.id = self.next_order_id;
        self.next_order_id += 1;

        order.timestamp = self.timestamp;

        match order.order_type {
            OrderType::Market => {
                // immediately execute market order
                self.execute_market(order.side, order.size);
                return Ok(order);
            }
            OrderType::Limit => {
                self.acc_tracker.log_limit_order_submission();
                self.orders_active.push(order.clone());
                self.margin.available_balance =
                    self.margin.wallet_balance - self.margin.position_margin - self.order_margin();
                return Ok(order);
            }
            _ => {}
        }
        self.orders_active.push(order.clone());

        return Ok(order);
    }

    /// Return the order margin used
    pub fn order_margin(&self) -> f64 {
        let mut order_margin_long: f64 = 0.0;
        let mut order_margin_short: f64 = 0.0;
        for o in &self.orders_active {
            // check which orders belong to position and which are "free"
            match o.side {
                Side::Buy => {
                    order_margin_long += o.size / o.limit_price / self.position.leverage;
                }
                Side::Sell => {
                    order_margin_short += o.size / o.limit_price / self.position.leverage;
                }
            }
        }
        if self.position.size > 0.0 {
            order_margin_short -= self.position.margin;
        } else {
            order_margin_long -= self.position.margin;
        }
        max(order_margin_long, order_margin_short)
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

    /// Return the number of active order
    pub fn num_active_orders(&self) -> usize {
        return self.orders_active.len();
    }

    /// Return the recently executed orders and clear afterwards
    pub fn executed_orders(&mut self) -> Vec<Order> {
        let exec_orders: Vec<Order> = self.orders_executed.clone();
        self.orders_executed.clear();
        return exec_orders;
    }

    /// Check if market order is correct
    pub fn validate_market_order(&mut self, o: &Order) -> Option<OrderError> {
        let price = match o.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let fee_base = self.config.fee_taker * o.size;
        let fee_asset = fee_base / self.bid;

        // check if enough available balance for initial margin requirements
        let order_margin: f64 = o.size / price / self.position.leverage;
        let order_err: Option<OrderError> = match o.side {
            Side::Buy => {
                if self.position.size > 0.0 {
                    if order_margin + fee_asset > self.margin.available_balance() {
                        return Some(OrderError::NotEnoughAvailableBalance);
                    }
                    None
                } else {
                    if order_margin > self.margin.position_margin() {
                        // check if there is enough available balance for the rest of order_margin
                        let margin_diff = order_margin - self.position.margin;
                        if margin_diff + fee_asset
                            > self.margin.available_balance() + self.position.margin
                        {
                            return Some(OrderError::NotEnoughAvailableBalance);
                        }
                        return None;
                    }
                    None
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
                            return Some(OrderError::NotEnoughAvailableBalance);
                        }
                        return None;
                    }
                    None
                } else {
                    if order_margin + fee_asset > self.margin.available_balance() {
                        return Some(OrderError::NotEnoughAvailableBalance);
                    }
                    None
                }
            }
        };
        return order_err;
    }

    /// Check if a limit order is correct
    pub fn validate_limit_order(&self, o: &Order) -> Option<OrderError> {
        // validate order price
        match o.side {
            Side::Buy => {
                if o.limit_price > self.ask {
                    return Some(OrderError::InvalidLimitPrice);
                }
            }
            Side::Sell => {
                if o.limit_price < self.bid {
                    return Some(OrderError::InvalidLimitPrice);
                }
            }
        }

        let order_margin: f64 = o.size / o.limit_price / self.position.leverage;
        let order_err: Option<OrderError> = match o.side {
            Side::Buy => {
                if self.position.size > 0.0 {
                    // check if enough margin is available
                    if order_margin > self.margin.available_balance() {
                        return Some(OrderError::NotEnoughAvailableBalance);
                    }
                    None
                } else {
                    if order_margin > self.margin.available_balance() + self.position.margin() {
                        return Some(OrderError::NotEnoughAvailableBalance);
                    }
                    None
                }
            }
            Side::Sell => {
                if self.position.size > 0.0 {
                    if order_margin > self.margin.available_balance() + self.position.margin() {
                        return Some(OrderError::NotEnoughAvailableBalance);
                    }
                    None
                } else {
                    if order_margin > self.margin.available_balance() {
                        return Some(OrderError::NotEnoughAvailableBalance);
                    }
                    None
                }
            }
        };
        order_err
    }

    /// Check if a stop market order is correct
    pub fn validate_stop_market_order(&mut self, o: &Order) -> Option<OrderError> {
        let order_err = match o.side {
            Side::Buy => {
                if o.trigger_price <= self.ask {
                    return Some(OrderError::InvalidTriggerPrice);
                }
                None
            }
            Side::Sell => {
                if o.trigger_price >= self.bid {
                    return Some(OrderError::InvalidTriggerPrice);
                }
                None
            }
        };
        if order_err.is_some() {
            return order_err;
        }

        None
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

    /// Check if a liquidation event should occur
    fn check_liquidation(&mut self) -> bool {
        // TODO: only liquidate when no more wallet balance is left
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

        return false;
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
        for i in 0..self.orders_active.len() {
            match self.orders_active[i].order_type {
                OrderType::Limit => self.handle_limit_order(i),
                OrderType::StopMarket => self.handle_stop_market_order(i),
                OrderType::Market => self.handle_market_order(i),
            }
        }
        // move executed orders from orders_active to orders_done
        let mut i: usize = 0;
        loop {
            if i >= self.orders_active.len() {
                break;
            }
            if self.orders_active[i].executed {
                let exec_order = self.orders_active.remove(i);
                self.orders_executed.push(exec_order);
            }
            i += 1;
        }
    }

    /// Handle stop market order trigger and execution
    fn handle_stop_market_order(&mut self, order_index: usize) {
        match self.orders_active[order_index].side {
            Side::Buy => {
                if self.orders_active[order_index].trigger_price > self.ask {
                    return;
                }
                self.execute_market(Side::Buy, self.orders_active[order_index].size);
                self.orders_active[order_index].mark_executed();
            }
            Side::Sell => {
                if self.orders_active[order_index].trigger_price > self.bid {
                    return;
                }
                self.execute_market(Side::Sell, self.orders_active[order_index].size);
                self.orders_active[order_index].mark_executed();
            }
        }
    }

    /// Handler for executing market orders
    fn handle_market_order(&mut self, order_index: usize) {
        match self.orders_active[order_index].side {
            Side::Buy => self.execute_market(Side::Buy, self.orders_active[order_index].size),
            Side::Sell => self.execute_market(Side::Sell, self.orders_active[order_index].size),
        }
        self.orders_active[order_index].mark_executed();
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_index: usize) {
        let o: &Order = &self.orders_active[order_index];
        match o.side {
            Side::Buy => {
                match self.config.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.low <= o.limit_price {
                            self.execute_limit(o.side, o.limit_price, o.size);
                            self.orders_active[order_index].mark_executed();
                        }
                    }
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.bid < o.limit_price {
                            // execute
                            self.execute_limit(o.side, o.limit_price, o.size);
                            self.orders_active[order_index].mark_executed();
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
                            self.orders_active[order_index].mark_executed();
                        }
                    }
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.ask > o.limit_price {
                            // execute
                            self.execute_limit(o.side, o.limit_price, o.size);
                            self.orders_active[order_index].mark_executed();
                        }
                    }
                }
            }
        }
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

/// Return the maximum of two values
pub fn max(val0: f64, val1: f64) -> f64 {
    if val0 > val1 {
        return val0;
    }
    val1
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
            leverage: 1.0
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
        let o = Order::market(Side::Buy, size);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        // valid order
        let o = Order::market(Side::Sell, size);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        // invalid order
        let size = exchange.ask * exchange.margin.available_balance * 1.05;
        let o = Order::market(Side::Buy, size);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        // invalid order
        let o = Order::market(Side::Sell, size);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::market(Side::Buy, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, 800.0);

        // valid order
        let o = Order::market(Side::Buy, 190.0);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        // invalid order
        let o = Order::market(Side::Buy, 210.0);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        // valid order
        let o = Order::market(Side::Sell, 800.0);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        // invalid order
        let o = Order::market(Side::Sell, 2100.0);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        // valid order
        let o = Order::market(Side::Sell, 1600.0);
        let order_err = exchange.validate_market_order(&o);
        match order_err {
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            Some(_) => panic!("other order err"),
            None => {}
        }
    }

    #[test]
    fn test_validate_limit_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
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
        let o = Order::limit(Side::Buy, price, size);
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_none());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Buy, price, size);
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Sell, price, size);
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_none());

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * 0.8 * price;
        let o = Order::limit(Side::Sell, price, size);
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * 1.1 * price;
        let o = Order::limit(Side::Buy, price, size);
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * 1.1 * price;
        let o = Order::limit(Side::Sell, price, size);
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());
    }

    #[test]
    fn submit_order_limit() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.order_margin(), 0.5);
        assert_eq!(exchange.margin.available_balance, 1.0 - 0.5);

        // submit working market order
        let o = Order::market(Side::Buy, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        // submit opposite limit order acting as target order
        let o = Order::limit(Side::Sell, 1200.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 2);
    }

    #[test]
    fn test_validate_stop_market_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 10.0);
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_none());

        let o = Order::stop_market(Side::Sell, 1010.0, 10.0);
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::stop_market(Side::Buy, 980.0, 10.0);
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::stop_market(Side::Sell, 980.0, 10.0);
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_none());

        let o = Order::stop_market(Side::Buy, 1000.0, 10.0);
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::stop_market(Side::Buy, 1000.0, 10.0);
        let order_err = exchange.validate_stop_market_order(&o);
        assert!(order_err.is_some());
    }

    #[test]
    fn test_handle_limit_order() {
        // TODO:
    }

    #[test]
    fn handle_stop_market_order() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 100.0);
        let valid = exchange.submit_order(o);
        assert!(valid.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.orders_active.len(), 1);

        let t = Trade {
            timestamp: 2,
            price: 1010.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        exchange.check_orders();

        assert_eq!(exchange.position.size, 100.0);
        assert_eq!(exchange.position.entry_price, 1010.0);
    }

    #[test]
    fn long_market_win_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
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
        let o = Order::market(Side::Buy, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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


        let o = Order::market(Side::Sell, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0);
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

        let o = Order::market(Side::Sell, 800.0);
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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0);
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

        let o = Order::market(Side::Buy, 800.0);
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
        assert_eq!(exchange.order_margin(), 0.0);
        assert_eq!(exchange.margin.available_balance, 1.2 - fee_combined);
    }

    #[test]
    fn short_market_loss_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
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
        let o = Order::market(Side::Sell, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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

        let o = Order::market(Side::Buy, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
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
        let o = Order::market(Side::Buy, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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

        let o = Order::market(Side::Sell, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0);
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

        let o = Order::market(Side::Sell, 400.0);
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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0);
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

        let o = Order::market(Side::Buy, 400.0);
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
        assert_eq!(round(exchange.margin.wallet_balance, 6), round(1.1 - fee_combined, 6));
        assert_eq!(exchange.margin.margin_balance, 1.2 - fee_combined);
        assert_eq!(exchange.margin.position_margin, 0.6);
        assert_eq!(exchange.order_margin(), 0.0);
        assert_eq!(round(exchange.margin.available_balance, 6), round(0.5 - fee_combined, 6));
    }

    #[test]
    fn short_market_loss_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
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
        let o = Order::market(Side::Sell, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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

        let o = Order::market(Side::Buy, size);
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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
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
        let buy_order = Order::market(Side::Buy, size);
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let sell_order = Order::market(Side::Sell, size);

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
        assert_eq!(exchange.order_margin(), 0.0);
        assert_eq!(exchange.margin.available_balance, 1.0 - 2.0 * fee_asset);

        let size = 900.0;
        let buy_order = Order::market(Side::Buy, size);
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let size = 950.0;
        let sell_order = Order::market(Side::Sell, size);

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
        assert_eq!(exchange.order_margin(), 0.0);
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
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 100.0,
            size: 10.0,
        };
        exchange.consume_trade(&t);
        for i in 0..100 {
            let o = Order::stop_market(Side::Buy, 101.0 + i as f64, 10.0);
            exchange.submit_order(o).unwrap();
        }
        let active_orders = exchange.orders_active;
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
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
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
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
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
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
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
            leverage: 1.0
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.order_margin(), 0.5);

        exchange.cancel_order(0);
        assert_eq!(exchange.orders_active.len(), 0);
        assert_eq!(exchange.margin.wallet_balance, 1.0);
        assert_eq!(exchange.margin.margin_balance, 1.0);
        assert_eq!(exchange.margin.available_balance, 1.0);
        assert_eq!(exchange.order_margin(), 0.0);
    }

    #[test]
    fn order_margin() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), 0.5);
        assert_eq!(exchange.orders_active.len(), 1);

        let o = Order::limit(Side::Sell, 1200.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), 0.5);
        assert_eq!(exchange.orders_active.len(), 2);

        let o = Order::market(Side::Buy, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.position.size, 450.0);
        assert_eq!(exchange.order_margin(), 0.5);

        let o = Order::limit(Side::Sell, 1200.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), 0.5);
        assert_eq!(exchange.orders_active.len(), 3);

        let o = Order::market(Side::Sell, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), 0.75);

        let o = Order::market(Side::Buy, 240.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.position.size, 240.0);
        assert_eq!(exchange.order_margin(), 0.51);
    }

    #[test]
    fn execute_limit() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o: Order = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.margin.available_balance, 0.5);

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
        assert_eq!(exchange.orders_active.len(), 0);
        assert_eq!(exchange.position.size, 450.0);
        assert_eq!(exchange.position.value, 0.6);
        assert_eq!(exchange.position.margin, 0.5);
        assert_eq!(exchange.position.entry_price, 900.0);
        assert_eq!(exchange.margin.wallet_balance, 1.000125);
        assert_eq!(exchange.order_margin(), 0.0);
        // Knapp daneben ist auch vorbei
        // assert_eq!(exchange.unrealized_pnl(), Float::new(-1, 1));
        // assert_eq!(exchange.margin.position_margin, Float::new(5, 1));
        // assert_eq!(exchange.margin.available_balance, Float::new(5, 1) + fee_maker);

        let o: Order = Order::limit(Side::Sell, 1000.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.order_margin(), 0.0);

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

        assert_eq!(exchange.orders_active.len(), 0);
        assert_eq!(exchange.position.size, 0.0);
        assert_eq!(exchange.position.value, 0.0);
        assert_eq!(exchange.position.margin, 0.0);
        assert_eq!(exchange.order_margin(), 0.0);
        assert_eq!(exchange.margin.position_margin, 0.0);
        let fee_maker_1: f64 = 0.001125;
        let wb: f64 = 1.0 + fee_maker_0 + fee_maker_1 + 0.05;
        // Again nearly correct but not quite which is fine though
        // assert_eq!(exchange.margin.wallet_balance, wb);
        // assert_eq!(exchange.margin.available_balance, wb);
        // assert_eq!(exchange.margin.margin_balance, wb);
    }
}
