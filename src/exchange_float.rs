extern crate trade_aggregation;

use trade_aggregation::common::*;
use crate::orders_float::*;
use crate::config_float::*;
use crate::acc_tracker::AccTracker;
use crate::{Side, OrderType, OrderError, FeeType};

#[derive(Debug, Clone)]
pub struct ExchangeFloat {
    pub config: ConfigFloat,
    pub position: PositionFloat,
    pub margin: MarginFloat,
    pub bid: f64,
    pub ask: f64,
    init: bool,
    pub rpnls: Vec<f64>,
    orders_done: Vec<OrderFloat>,
    orders_executed: Vec<OrderFloat>,
    pub orders_active: Vec<OrderFloat>,
    next_order_id: u64,
    pub acc_tracker: AccTracker,
    timestamp: u64,  // used for syncronizing orders
    high: f64,
    low: f64,
    use_candles: bool,
}

#[derive(Debug, Clone)]
pub struct MarginFloat {
    pub wallet_balance: f64,
    pub margin_balance: f64,
    pub position_margin: f64,
    pub order_margin: f64,
    pub available_balance: f64,
}

#[derive(Debug, Clone)]
pub struct PositionFloat {
    pub size: f64,
    pub value: f64,  // value of positoin in units of quoteCurrency
    pub entry_price: f64,
    pub liq_price: f64,
    pub margin: f64,
    pub leverage: f64,
    pub unrealized_pnl: f64,
}

impl ExchangeFloat {

    pub fn new(config: ConfigFloat, use_candles: bool) -> ExchangeFloat {
        return ExchangeFloat {
            config,
            position: PositionFloat{
                size: 0.0,
                value: 0.0,
                entry_price: 0.0,
                liq_price: 0.0,
                margin: 0.0,
                leverage: 1.0,
                unrealized_pnl: 0.0,
            },
            margin: MarginFloat{
                wallet_balance: 1.0,
                margin_balance: 1.0,
                position_margin: 0.0,
                order_margin: 0.0,
                available_balance: 1.0,
            },
            bid: 0.0,
            ask: 0.0,
            init: true,
            rpnls: Vec::new(),
            orders_done: Vec::new(),
            orders_executed: Vec::new(),
            orders_active: Vec::new(),
            next_order_id: 0,
            acc_tracker: AccTracker::new(1.0),
            timestamp: 0,
            high: 0.0,
            low: 0.0,
            use_candles,
        }
    }

    // sets the new leverage of position
    // returns true if successful
    pub fn set_leverage(&mut self, l: f64) -> bool {
        if l < 1.0 {
            return false
        }

        let new_position_margin = (self.position.value / l) + self.position.unrealized_pnl;
        if new_position_margin > self.margin.wallet_balance {
            return false
        }
        self.position.leverage = l;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.unrealized_pnl();
        self.margin.available_balance = self.margin.margin_balance - self.margin.order_margin - self.margin.position_margin;
        self.position.margin = self.position.value / self.position.leverage;

        return true
    }

    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    // consume_candle update the exchange state with th new candle.
    // returns true if position has been liquidated
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

        if self.check_liquidation() {
            return true
        }

        self.check_orders();

        return false
    }

    // consume_candle update the bid and ask price given a candle using its close price
    // returns true if position has been liquidated
    pub fn consume_candle(&mut self, candle: &Candle) -> bool {
        self.bid = candle.close;
        self.ask = candle.close;
        self.high = candle.high;
        self.low = candle.low;

        if self.check_liquidation() {
            return true
        }

        self.check_orders();

        return false
    }

    // cancels an active order
    // returns true if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<OrderFloat> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.orders_active.remove(i);
                self.update_position_stats();
                return Some(old_order);
            }
        }
        None
    }

    pub fn query_active_orders(&self, order_id: u64) -> Option<&OrderFloat> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                return self.orders_active.get(i);
            }
        }
        None
    }

    // submits the order to the exchange
    // returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: OrderFloat) -> Result<OrderFloat, OrderError> {
        match order.order_type {
            OrderType::StopMarket => {
                if self.orders_active.len() >= 10 {
                    return Err(OrderError::MaxActiveOrders )
                }
            },
            _ => {
                if self.orders_active.len() >= 200 {
                    return Err(OrderError::MaxActiveOrders)
                }
            }
        }
        if order.size <= 0.0 {
            return Err(OrderError::InvalidOrderSize)
        }
        let order_err: Option<OrderError> = match order.order_type {
            OrderType::Market => self.validate_market_order(&order),
            OrderType::Limit => self.validate_limit_order(&order),
            OrderType::StopMarket => self.validate_stop_market_order(&order),
            OrderType::TakeProfitLimit => self.validate_take_profit_limit_order(&order),
            OrderType::TakeProfitMarket => self.validate_take_profit_market_order(&order),
        };
        if order_err.is_some() {
            return Err(order_err.unwrap())
        }

        // assign unique order id
        order.id = self.next_order_id;
        self.next_order_id += 1;

        order.timestamp = self.timestamp;

        match order.order_type {
            OrderType::Market => {
                // immediately execute market order
                self.execute_market(order.side, order.size);
                return Ok(order)
            }
            OrderType::Limit => {
                self.acc_tracker.log_limit_order_submission();
                self.orders_active.push(order.clone());
                self.margin.available_balance = self.margin.wallet_balance - self.margin.position_margin - self.order_margin();
                return Ok(order)
            }
            _ => {},
        }
        self.orders_active.push(order.clone());

        return Ok(order)
    }

    pub fn order_margin(&self) -> f64 {
        let mut order_margin_long: f64 = 0.0;
        let mut order_margin_short: f64 = 0.0;
        for o in &self.orders_active {
            // check which orders belong to position and which are "free"
            match o.side {
                Side::Buy => {
                    order_margin_long += o.size / o.price / self.position.leverage;
                },
                Side::Sell => {
                    order_margin_short += o.size / o.price / self.position.leverage;
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

    pub fn unrealized_pnl(&self) -> f64 {
        return if self.position.size == 0.0 {
            0.0
        } else if self.position.size > 0.0 {
            ((1.0 / self.position.entry_price) - (1.0 / self.bid)) * self.position.size.abs()
        } else {
            ((1.0 / self.ask) - (1.0 / self.position.entry_price)) * self.position.size.abs()
        }
    }

    pub fn num_active_orders(&self) -> usize {
        return self.orders_active.len()
    }

    pub fn executed_orders(&mut self) -> Vec<OrderFloat> {
        let exec_orders: Vec<OrderFloat> = self.orders_executed.clone();
        // move to orders_done if needed
        // for o in &exec_orders {
        //     self.orders_done.push(o);
        // }
        // clear executed orders
        self.orders_executed.clear();
        return exec_orders
    }

    pub fn ammend_order(&mut self, _order_id: u64, _new_order: OrderFloat) -> Option<OrderError> {
        // TODO: exchange_float: ammend_order
        unimplemented!("exchange_float ammend_order is not implemented yet");
    }

    // check if market order is correct
    pub fn validate_market_order(&mut self, o: &OrderFloat) -> Option<OrderError> {
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
                    if order_margin + fee_asset > self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                } else {
                    if order_margin > self.margin.position_margin {
                        // check if there is enough available balance for the rest of order_margin
                        let margin_diff = order_margin - self.position.margin;
                        if margin_diff + fee_asset > self.margin.available_balance + self.position.margin {
                            return Some(OrderError::NotEnoughAvailableBalance)
                        }
                        return None
                    }
                    None
                }
            },
            Side::Sell => {
                if self.position.size > 0.0 {
                    if order_margin > self.margin.position_margin {
                        // check if there is enough available balance for the rest of order_margin
                        let margin_diff = order_margin - self.position.margin;
                        if margin_diff + fee_asset > self.margin.available_balance + self.position.margin {
                            return Some(OrderError::NotEnoughAvailableBalance)
                        }
                        return None
                    }
                    None
                } else {
                    if order_margin + fee_asset > self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                }
            }
        };
        return order_err
    }

    pub fn validate_limit_order(&self, o: &OrderFloat) -> Option<OrderError> {
        // validate order price
        match o.side {
            Side::Buy => {
                if o.price > self.ask {
                    return Some(OrderError::InvalidPrice)
                }
            },
            Side::Sell => {
                if o.price < self.bid {
                    return Some(OrderError::InvalidPrice)
                }
            },
        }

        let order_margin: f64 = o.size / o.price / self.position.leverage;
        let order_err: Option<OrderError> = match o.side {
            Side::Buy => {
                if self.position.size > 0.0 {
                    // check if enough margin is available
                    if order_margin > self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None

                } else {
                    if order_margin > self.margin.available_balance + self.position.margin {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                }
            },
            Side::Sell => {
                if self.position.size > 0.0 {
                    if order_margin > self.margin.available_balance + self.position.margin {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                } else {
                    if order_margin > self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                }
            },
        };
        order_err
    }

    pub fn validate_take_profit_limit_order(&self, _o: &OrderFloat) -> Option<OrderError> {
        // TODO: exchange_float: validate_take_profit_limit_order
        unimplemented!("exchange_float: validate_take_profit_limit_order is not implemented yet");
    }

    // returns true if order is valid
    pub fn validate_stop_market_order(&mut self, o: &OrderFloat) -> Option<OrderError> {
        let order_err =  match o.side {
            Side::Buy => { if o.price <= self.ask { return Some(OrderError::InvalidTriggerPrice) }
                None
            },
            Side::Sell => { if o.price >= self.bid { return Some(OrderError::InvalidTriggerPrice) }
                None
            },
        };
        if order_err.is_some() {
            return order_err
        }

        None
    }

    pub fn validate_take_profit_market_order(&self, o: &OrderFloat) -> Option<OrderError> {
        return match o.side {
            Side::Buy => { if o.price > self.bid { return Some(OrderError::InvalidOrder) }
                None
            },
            Side::Sell => { if o.price < self.ask { return Some(OrderError::InvalidOrder) }
                None
            },
        }
    }

    pub fn roe(&self) -> f64 {
        return if self.position.size > 0.0 {
            (self.bid - self.position.entry_price) / self.position.entry_price
        } else {
            (self.position.entry_price - self.ask) / self.position.entry_price
        }
    }

    fn check_liquidation(&mut self) -> bool {
        // TODO: only liquidate when no more wallet balance is left
        if self.position.size > 0.0 {
            // liquidation check for long position
            if self.ask < self.position.liq_price {
                self.liquidate();
                return true
            }
            self.position.unrealized_pnl = self.unrealized_pnl();

        } else if self.position.size < 0.0 {
            // liquidation check for short position
            if self.bid > self.position.liq_price {
                self.liquidate();
                return true
            }
            self.position.unrealized_pnl = self.unrealized_pnl();
        }

        return false
    }

    fn deduce_fees(&mut self, t: FeeType, amount_base: f64, price: f64) {
        let fee: f64 = match t {
            FeeType::Maker => self.config.fee_maker,
            FeeType::Taker => self.config.fee_taker,
        };
        let fee_base = fee * amount_base;
        let fee_asset = fee_base / price;
        self.margin.wallet_balance -= fee_asset;
        self.update_position_stats();
    }

    fn update_position_stats(&mut self) {
        let price: f64 = if self.position.size > 0.0 {
            self.bid
        } else {
            self.ask
        };
        self.position.unrealized_pnl = self.unrealized_pnl();
        self.position.value = self.position.size.abs() / price;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
        self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;
        self.margin.available_balance = self.margin.wallet_balance - self.margin.position_margin - self.margin.order_margin;
    }

    fn execute_market(&mut self, side: Side, amount_base: f64) {
        let price: f64 = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        self.deduce_fees(FeeType::Taker, amount_base, price);

        let old_position_size = self.position.size;
        let old_entry_price: f64 =  if self.position.size == 0.0 {
            price
        } else {
            self.position.entry_price
        };
        let upnl = self.unrealized_pnl();
        self.acc_tracker.log_trade(side, amount_base, upnl);

        match side {
            Side::Buy => {
                if self.position.size < 0.0 {
                    if amount_base >= self.position.size.abs() {
                        // realize_pnl
                        let rpnl = self.position.size.abs() * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;

                    } else {
                        // realize pnl
                        let rpnl = amount_base * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size += amount_base;
                        self.position.margin = self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.margin += amount_base / old_entry_price/ self.position.leverage;
                    self.position.entry_price = ((price * amount_base) + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            },
            Side::Sell => {
                if self.position.size > 0.0 {
                    if amount_base >= self.position.size.abs() {
                        // realize pnl
                        let rpnl = self.position.size.abs() * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;

                    } else {
                        // realize pnl
                        let rpnl = amount_base * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size -= amount_base;
                        self.position.margin = self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size -= amount_base;
                    self.position.margin += amount_base / old_entry_price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base) + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            },
        }

        self.update_position_stats();
        self.update_liq_price();
    }

    fn execute_limit(&mut self, side: Side, price: f64, amount_base: f64) {
        self.acc_tracker.log_limit_order_fill();
        self.deduce_fees(FeeType::Maker, amount_base, price);
        let upnl = self.unrealized_pnl();
        self.acc_tracker.log_trade(side, amount_base, upnl);

        let old_position_size = self.position.size;
        let old_entry_price: f64 =  if self.position.size == 0.0 {
            price
        } else {
            self.position.entry_price
        };

        match side {
            Side::Buy => {
                if self.position.size < 0.0 {
                    // realize pnl
                    if amount_base > self.position.size.abs() {
                        let rpnl = self.position.size.abs() * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        let rpnl = amount_base * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size += amount_base;
                        self.position.margin = self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.margin += amount_base / price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base) + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            },
            Side::Sell => {
                if self.position.size > 0.0 {
                    // realize pnl
                    if amount_base > self.position.size.abs() {
                        let rpnl = self.position.size.abs() * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        let rpnl = amount_base * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.rpnls.push(rpnl);
                        self.acc_tracker.log_rpnl(rpnl);

                        self.position.size -= amount_base;
                        self.position.margin = self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.margin += amount_base / price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base) + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            }
        }

        self.update_position_stats();
        self.update_liq_price();
    }

    fn liquidate(&mut self) {
        debug!("liquidating");
        if self.position.size > 0.0 {
            self.execute_market(Side::Sell, self.position.size);
        } else {
            self.execute_market(Side::Buy, self.position.size);
        }

        self.update_position_stats();
    }

    fn check_orders(&mut self) {
        for i in 0..self.orders_active.len() {
            match self.orders_active[i].order_type {
                OrderType::Limit => self.handle_limit_order(i),
                OrderType::StopMarket => self.handle_stop_market_order(i),
                OrderType::Market => self.handle_market_order(i),
                OrderType::TakeProfitLimit => self.handle_take_profit_limit_order(i),
                OrderType::TakeProfitMarket => self.handle_take_profit_market_order(i),
            }
        }
        // move executed orders from orders_active to orders_done
        let mut i: usize = 0;
        loop {
            if i >= self.orders_active.len() {
                break
            }
            if self.orders_active[i].done() {
                let exec_order = self.orders_active.remove(i);
                self.orders_executed.push(exec_order);
            }
            i += 1;
        }
    }

    fn handle_stop_market_order(&mut self, order_index: usize) {
        match self.orders_active[order_index].side {
            Side::Buy => {
                if self.orders_active[order_index].price > self.ask { return }
                self.execute_market(Side::Buy, self.orders_active[order_index].size);
                self.orders_active[order_index].mark_done();
            },
            Side::Sell => {
                if self.orders_active[order_index].price > self.bid { return }
                self.execute_market(Side::Sell, self.orders_active[order_index].size);
                self.orders_active[order_index].mark_done();
            },
        }
    }

    fn handle_take_profit_market_order(&mut self, order_index: usize) {
        match self.orders_active[order_index].side {
            Side::Buy => { if self.orders_active[order_index].price < self.bid { return }
                self.execute_market(Side::Buy, self.orders_active[order_index].size * self.ask);
                self.orders_active[order_index].mark_done();
            },
            Side::Sell => { if self.orders_active[order_index].price > self.ask { return }
                self.execute_market(Side::Sell, self.orders_active[order_index].size * self.bid);
                self.orders_active[order_index].mark_done();
            },
        }
    }

    fn handle_market_order(&mut self, order_index: usize) {
        match self.orders_active[order_index].side {
            Side::Buy => self.execute_market(Side::Buy, self.orders_active[order_index].size),
            Side::Sell => self.execute_market(Side:: Sell, self.orders_active[order_index].size),
        }
        self.orders_active[order_index].mark_done();
    }

    fn handle_limit_order(&mut self, order_index: usize) {
        let o: &OrderFloat = &self.orders_active[order_index];
        match o.side {
            Side::Buy => {
                match self.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.low <= o.price {
                            self.execute_limit(o.side, o.price, o.size);
                            self.orders_active[order_index].mark_done();
                        }
                    },
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.bid < o.price {
                            // execute
                            self.execute_limit(o.side, o.price, o.size);
                            self.orders_active[order_index].mark_done();
                        }
                    }
                }
            },
            Side::Sell => {
                match self.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.high >= o.price {
                            self.execute_limit(o.side, o.price, o.size);
                            self.orders_active[order_index].mark_done();
                        }
                    },
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.ask > o.price {
                            // execute
                            self.execute_limit(o.side, o.price, o.size);
                            self.orders_active[order_index].mark_done();
                        }
                    }
                }
                
            }
        }
    }

    fn handle_take_profit_limit_order(&mut self, _order_index: usize) {
        // TODO: exchange_float: handle_take_profit_limit_order
        unimplemented!("exchange_float: handle_take_profit_limit_order is not implemented yet");
    }

    fn update_liq_price(&mut self) {
        if self.position.size == 0.0 {
            self.position.liq_price = 0.0;
        } else if self.position.size > 0.0 {
            self.position.liq_price = self.position.entry_price - (self.position.entry_price / self.position.leverage);
        } else {
            self.position.liq_price = self.position.entry_price + (self.position.entry_price / self.position.leverage);
        }
    }

}

pub fn min(val0: f64, val1: f64) -> f64 {
    if val0 < val1 {
        return val0
    }
    return val1
}

pub fn max(val0: f64, val1: f64) -> f64 {
    if val0 > val1 {
        return val0
    }
    val1
}

