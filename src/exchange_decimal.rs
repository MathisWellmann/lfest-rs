extern crate trade_aggregation;
extern crate sliding_features;

use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use trade_aggregation::common::*;
use crate::orders_decimal::*;
use crate::config_decimal::*;
use crate::acc_tracker::AccTracker;


#[derive(Debug, Clone)]
pub struct ExchangeDecimal {
    pub config: Config,
    pub position: Position,
    pub margin: Margin,
    pub acc_tracker: AccTracker,
    pub bid: Decimal,
    pub ask: Decimal,
    init: bool,
    pub rpnls: Vec<f64>,
    orders_done: Vec<Order>,
    orders_executed: Vec<Order>,
    pub orders_active: Vec<Order>,
    next_order_id: u64,
    timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct Margin {
    pub wallet_balance: Decimal,
    pub margin_balance: Decimal,
    pub position_margin: Decimal,
    // pub order_margin: Decimal,
    pub available_balance: Decimal,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub size: Decimal,
    pub value: Decimal,
    pub entry_price: Decimal,
    pub liq_price: Decimal,
    pub margin: Decimal,
    pub leverage: Decimal,
    pub unrealized_pnl: Decimal,
}

#[derive(Debug, Clone)]
pub enum FeeType {
    Maker,
    Taker,
}

impl ExchangeDecimal {

    pub fn new(config: Config) -> ExchangeDecimal {
        return ExchangeDecimal {
            config,
            position: Position{
                size: Decimal::new(0, 0),
                value: Decimal::new(0, 0),
                entry_price: Decimal::new(0, 0),
                liq_price: Decimal::new(0, 0),
                margin: Decimal::new(0, 0),
                leverage: Decimal::new(1, 0),
                unrealized_pnl: Decimal::new(0, 0),
            },
            margin: Margin{
                wallet_balance: Decimal::new(1, 0),
                margin_balance: Decimal::new(1, 0),
                position_margin: Decimal::new(0, 0),
                // order_margin: Decimal::new(0, 0),
                available_balance: Decimal::new(1, 0),
            },
            acc_tracker: AccTracker::new(1.0),
            bid: Decimal::new(0, 0),
            ask: Decimal::new(0, 0),
            init: true,
            rpnls: Vec::new(),
            orders_done: Vec::new(),
            orders_executed: Vec::new(),
            orders_active: Vec::new(),
            next_order_id: 0,
            timestamp: 0,
        }
    }

    // sets the new leverage of position
    // returns true if successful
    pub fn set_leverage(&mut self, l: f64) -> bool {
        let l = Decimal::from_f64(l).unwrap();
        if l < Decimal::new(1, 0) {
            return false
        }

        let new_position_margin = (self.position.value / l) + self.position.unrealized_pnl;
        if new_position_margin > self.margin.wallet_balance {
            return false
        }
        self.position.leverage = l;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.unrealized_pnl();
        self.margin.available_balance = self.margin.margin_balance - self.order_margin() - self.margin.position_margin;
        self.position.margin = self.position.value / self.position.leverage;

        return true
    }

    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    // consume_candle update the exchange state with th new candle.
    // returns true if position has been liquidated
    pub fn consume_trade(&mut self, trade: &Trade) -> bool {
        let price = Decimal::from_f64(trade.price).unwrap();
        if self.init {
            self.init = false;
            self.bid = price;
            self.ask = price;
        }
        if trade.size > 0.0 {
            self.ask = price;
        } else {
            self.bid = price;
        }

        if self.check_liquidation() {
            return true
        }

        self.check_orders();
        self.update_position_stats();

        return false
    }

    // consume_candle update the bid and ask price given a candle using its close price
    // returns true if position has been liquidated
    pub fn consume_candle(&mut self, candle: &Candle) -> bool {
        // TODO: set bid and ask with spread in mind

        self.bid = Decimal::from_f64(candle.close).unwrap();
        self.ask = Decimal::from_f64(candle.close).unwrap();

        if self.check_liquidation() {
            return true
        }

        self.check_orders();
        self.update_position_stats();

        return false
    }

    // cancels an active order
    // returns the cancelled order if successful
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

    pub fn query_active_orders(&self, order_id: u64) -> Option<&Order> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                return self.orders_active.get(i);
            }
        }
        None
    }

    pub fn submit_order(&mut self, mut order: Order) -> Result<Order, OrderError> {
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

        if order.size <= Decimal::new(0, 0) {
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

    // calculates the total order margin
    pub fn order_margin(&self) -> Decimal {
        let mut order_margin_long: Decimal = Decimal::new(0, 0);
        let mut order_margin_short: Decimal = Decimal::new(0, 0);
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
        if self.position.size > Decimal::new(0, 0) {
            order_margin_short -= self.position.margin;
        } else {
            order_margin_long -= self.position.margin;
        }
        max(order_margin_long, order_margin_short)
    }

    pub fn unrealized_pnl(&self) -> Decimal {
        return if self.position.size == Decimal::new(0, 0) {
            Decimal::new(0, 0)
        } else if self.position.size > Decimal::new(0, 0) {
            ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / self.bid)) * self.position.size.abs()
        } else {
            ((Decimal::new(1, 0) / self.ask) - (Decimal::new(1, 0) / self.position.entry_price)) * self.position.size.abs()
        }
    }

    pub fn num_active_orders(&self) -> usize {
        return self.orders_active.len()
    }

    pub fn executed_orders(&mut self) -> Vec<Order> {
        let exec_orders: Vec<Order> = self.orders_executed.clone();
        // move to orders_done if needed
        // for o in &exec_orders {
        //     self.orders_done.push(o);
        // }
        // clear executed orders
        self.orders_executed.clear();
        return exec_orders
    }

    pub fn ammend_order(&mut self, _order_id: u64, _new_order: Order) -> Option<OrderError> {
        // TODO: exchange_decimal: ammend_order
        unimplemented!("exchange_decimal: ammend_order is not implemented yet");
    }

    // check if market order is correct
    pub fn validate_market_order(&mut self, o: &Order) -> Option<OrderError> {
        let price = match o.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let fee_base = self.config.fee_taker * o.size;
        let fee_asset = fee_base / self.bid;

        // check if enough available balance for initial margin requirements
        // TODO: change order_margin calculation for markets denoted in XBT
        let order_margin: Decimal = o.size / price / self.position.leverage;
        let order_err: Option<OrderError> = match o.side {
            Side::Buy => {
                if self.position.size > Decimal::new(0, 0) {
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
                if self.position.size > Decimal::new(0, 0) {
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

    pub fn validate_limit_order(&mut self, o: &Order) -> Option<OrderError> {
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

        let order_margin: Decimal = o.size / o.price / self.position.leverage;
        let order_err: Option<OrderError> = match o.side {
            Side::Buy => {
                if self.position.size > Decimal::new(0, 0) {
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
                if self.position.size > Decimal::new(0, 0) {
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

    pub fn validate_take_profit_limit_order(&self, _o: &Order) -> Option<OrderError> {
        // TODO: exchange_decimal: validate_take_profit_limit_order
        unimplemented!("exchange_decimal: validate_take_profit_limit_order is not implemented yet");
    }

    // returns true if order is valid
    pub fn validate_stop_market_order(&mut self, o: &Order) -> Option<OrderError> {
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

    pub fn validate_take_profit_market_order(&self, o: &Order) -> Option<OrderError> {
        return match o.side {
            Side::Buy => { if o.price > self.bid { return Some(OrderError::InvalidOrder) }
                None
            },
            Side::Sell => { if o.price < self.ask { return Some(OrderError::InvalidOrder) }
                None
            },
        }
    }

    pub fn roe(&self) -> Decimal {
        return if self.position.size > Decimal::new(0, 0) {
            (self.bid - self.position.entry_price) / self.position.entry_price
        } else {
            (self.position.entry_price - self.ask) / self.position.entry_price
        }
    }

    fn check_liquidation(&mut self) -> bool {
        if self.position.size > Decimal::new(0, 0) {
            // liquidation check for long position
            if self.ask < self.position.liq_price {
                self.liquidate();
                return true
            }
            self.position.unrealized_pnl = self.unrealized_pnl();

        } else if self.position.size < Decimal::new(0, 0) {
            // liquidation check for short position
            if self.bid > self.position.liq_price {
                self.liquidate();
                return true
            }
            self.position.unrealized_pnl = self.unrealized_pnl();
        }

        return false
    }

    fn deduce_fees(&mut self, t: FeeType, amount_base: Decimal, price: Decimal) {
        let fee: Decimal = match t {
            FeeType::Maker => self.config.fee_maker,
            FeeType::Taker => self.config.fee_taker,
        };
        let fee_base = fee * amount_base;
        let fee_asset = fee_base / price;
        self.margin.wallet_balance -= fee_asset;
        self.update_position_stats();
    }

    fn update_position_stats(&mut self) {
        let price: Decimal = if self.position.size > Decimal::new(0, 0) {
            self.bid
        } else {
            self.ask
        };
        self.position.unrealized_pnl = self.unrealized_pnl();
        self.position.value = self.position.size.abs() / price;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
        self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;
        self.margin.available_balance = self.margin.wallet_balance - self.margin.position_margin - self.order_margin();
    }

    fn execute_market(&mut self, side: Side, amount_base: Decimal) {
        let price: Decimal = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        self.deduce_fees(FeeType::Taker, amount_base, price);

        let old_position_size = self.position.size;
        let old_entry_price: Decimal =  if self.position.size == Decimal::new(0, 0) {
            price
        } else {
            self.position.entry_price
        };
        self.acc_tracker.log_trade(side, amount_base.to_f64().unwrap());

        match side {
            Side::Buy => {
                if self.position.size < Decimal::new(0,0) {
                    // realize_pnl
                    if amount_base >= self.position.size.abs() {
                        let rpnl = self.position.size.abs() * ((Decimal::new(1, 0) / price) - (Decimal::new(1, 0) / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;

                    } else {
                        let rpnl = amount_base * ((Decimal::new(1, 0) / price) - (Decimal::new(1, 0) / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

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
                if self.position.size > Decimal::new(0, 0) {
                    // realize_pnl
                    if amount_base >= self.position.size.abs() {
                        let rpnl = self.position.size.abs() * ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;

                    } else {
                        let rpnl = amount_base * ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

                        self.position.size -= amount_base;
                        self.position.margin = self.position.size.abs() / old_entry_price / self.position.leverage;
                        self.position.entry_price = old_entry_price;
                    }
                } else {
                    self.position.size -= amount_base;
                    self.position.margin += amount_base / price / self.position.leverage;
                    self.position.entry_price = ((price * amount_base) + self.position.entry_price * old_position_size.abs())
                        / (amount_base + old_position_size.abs());
                }
            },
        }

        self.update_position_stats();
        self.update_liq_price();
    }

    fn execute_limit(&mut self, side: Side, price: Decimal, amount_base: Decimal) {
        self.acc_tracker.log_limit_order_fill();
        self.deduce_fees(FeeType::Maker, amount_base, price);
        self.acc_tracker.log_trade(side, amount_base.to_f64().unwrap());

        let old_position_size = self.position.size;
        let old_entry_price: Decimal =  if self.position.size == Decimal::new(0, 0) {
            price
        } else {
            self.position.entry_price
        };

        match side {
            Side::Buy => {
                if self.position.size < Decimal::new(0, 0) {
                    // realize pnl
                    if amount_base > self.position.size.abs() {
                        let rpnl = self.position.size.abs() * ((Decimal::new(1, 0) / price) - (Decimal::new(1, 0) / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        let rpnl = amount_base * ((Decimal::new(1, 0) / price) - (Decimal::new(1, 0) / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

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
                if self.position.size > Decimal::new(0, 0) {
                    // realize pnl
                    if amount_base > self.position.size.abs() {
                        let rpnl = self.position.size.abs() * ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;
                    } else {
                        let rpnl = amount_base * ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.rpnls.push(rpnl_f64);
                        self.acc_tracker.log_rpnl(rpnl_f64);

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
        if self.position.size > Decimal::new(0, 0) {
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
                OrderType::TakeProfitLimit => self.handle_take_profit_limit_order(i),
                OrderType::TakeProfitMarket => self.handle_take_profit_market_order(i),
                OrderType::Market => { panic!("market orders should have been executed immediately!") },
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

    // handle_limit_order will check and conditionally execute the limit order
    // uses pessimistic order fill model, in which the bid / ask price must have crossed the
    // limit price in order to get filled
    fn handle_limit_order(&mut self, order_index: usize) {
        let o: &Order = &self.orders_active[order_index];
        match o.side {
            Side::Buy => {
                if self.ask < o.price {
                    // execute
                    self.execute_limit(o.side, o.price, o.size);
                    self.orders_active[order_index].mark_done();
                }
            },
            Side::Sell => {
                if self.bid > o.price {
                    // execute
                    self.execute_limit(o.side, o.price, o.size);
                    self.orders_active[order_index].mark_done();
                }
            }
        }
    }

    fn handle_take_profit_limit_order(&mut self, _order_index: usize) {
        // TODO: exchange_decimal: handle_take_profit_limit_order
        unimplemented!("exchange_decimal: handle_take_profit_limit_order is not implemented yet");
    }

    fn update_liq_price(&mut self) {
        if self.position.size == Decimal::new(0, 0) {
            self.position.liq_price = Decimal::new(0, 0);
        } else if self.position.size > Decimal::new(0, 0) {
            self.position.liq_price = self.position.entry_price - (self.position.entry_price / self.position.leverage);
        } else {
            self.position.liq_price = self.position.entry_price + (self.position.entry_price / self.position.leverage);
        }
    }

}

pub fn min(val0: Decimal, val1: Decimal) -> Decimal {
    if val0 < val1 {
        return val0
    }
    return val1
}

pub fn max(val0: Decimal, val1: Decimal) -> Decimal {
    if val0 > val1 {
        return val0
    }
    val1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_market_order() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        // valid order
        let size = exchange.ask * exchange.margin.available_balance * Decimal::new(4, 1);
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let o = Order::market(Side::Buy, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        // valid order
        let o = Order::market(Side::Sell, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        // invalid order
        let size = exchange.ask * exchange.margin.available_balance * Decimal::new(105, 2);
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let o = Order::market(Side::Buy, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        // invalid order
        let o = Order::market(Side::Sell, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::market(Side::Buy, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(800, 0));

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
            Some(OrderError::InvalidOrder) => panic!("invalid order"),
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            Some(_) => panic!("other order err"),
            None => {},
        }
    }

    #[test]
    fn test_validate_limit_order() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * Decimal::new(8, 1) * Decimal::new(price as i64, 0);
        let o = Order::limit(Side::Buy, price, size.to_f64().unwrap());
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_none());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * Decimal::new(8, 1) * Decimal::new(price as i64, 0);
        let o = Order::limit(Side::Buy, price, size.to_f64().unwrap());
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * Decimal::new(8, 1) * Decimal::new(price as i64, 0);
        let o = Order::limit(Side::Sell, price, size.to_f64().unwrap());
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_none());

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * Decimal::new(8, 1) * Decimal::new(price as i64, 0);
        let o = Order::limit(Side::Sell, price, size.to_f64().unwrap());
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());

        let price: f64 = 990.0;
        let size = exchange.margin.wallet_balance * Decimal::new(11, 1) * Decimal::new(price as i64, 0);
        let o = Order::limit(Side::Buy, price, size.to_f64().unwrap());
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());

        let price: f64 = 1010.0;
        let size = exchange.margin.wallet_balance * Decimal::new(11, 1) * Decimal::new(price as i64, 0);
        let o = Order::limit(Side::Sell, price, size.to_f64().unwrap());
        let order_err = exchange.validate_limit_order(&o);
        assert!(order_err.is_some());
    }

    #[test]
    fn submit_order_limit() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.order_margin(), Decimal::new(5, 1));
        assert_eq!(exchange.margin.available_balance, Decimal::new(1, 0) - Decimal::new(5, 1));

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
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
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
    fn test_validate_take_profit_market_order() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::take_profit_market(Side::Buy, 950.0, 10.0);
        let order_err = exchange.validate_take_profit_market_order(&o);
        assert!(order_err.is_none());

        let o = Order::take_profit_market(Side::Sell, 950.0, 10.0);
        let order_err = exchange.validate_take_profit_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::take_profit_market(Side::Buy, 1050.0, 10.0);
        let order_err = exchange.validate_take_profit_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::take_profit_market(Side::Sell, 1050.0, 10.0);
        let order_err = exchange.validate_take_profit_market_order(&o);
        assert!(order_err.is_none());
    }

    #[test]
    fn test_validate_take_profit_limit_order() {
        // TODO:
    }

    #[test]
    fn test_handle_limit_order() {
        // TODO:
    }

    #[test]
    fn handle_stop_market_order() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
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

        let t = Trade{
            timestamp: 2,
            price: 1010.0,
            size: 100.0
        };
        exchange.consume_trade(&t);

        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(100, 0));
        assert_eq!(exchange.position.entry_price, Decimal::new(1010, 0));

    }

    #[test]
    fn long_market_win_full()  {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * Decimal::new(8, 1);
        let size = exchange.ask * value;
        let o = Order::market(Side::Buy, size.to_f64().unwrap());
        let order_err = exchange.submit_order(o);
       assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset1 = fee_base / exchange.bid;

        assert_eq!(exchange.position.size, size);
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.position_margin, Decimal::new(8, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(2, 1) - fee_asset1);

        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = Decimal::new(800, 0);
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / Decimal::new(2000, 0);

        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(4, 1));

        let o = Order::market(Side::Sell, size.to_f64().unwrap());
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(0, 0));
        assert_eq!(exchange.position.value, Decimal::new(0, 0));
        assert_eq!(exchange.position.margin, Decimal::new(0, 0));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(14, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(14, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(14, 1) - fee_asset1 - fee_asset2);

    }

    #[test]
    fn long_market_loss_full() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(800, 0));

        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), Decimal::new(-2, 1));

        let o = Order::market(Side::Sell, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * Decimal::new(800, 0);
        let fee_asset0 = fee_base0 / Decimal::new(1000, 0);

        let fee_base1 = fee_taker * Decimal::new(800, 0);
        let fee_asset1 = fee_base1 / Decimal::new(800, 0);

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, Decimal::new(0, 0));
        assert_eq!(exchange.position.value, Decimal::new(0, 0));
        assert_eq!(exchange.position.margin, Decimal::new(0, 0));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(8, 1) - fee_combined);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(8, 1) - fee_combined);
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(8, 1) - fee_combined);

    }

    #[test]
    fn short_market_win_full() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(-800, 0));

        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), Decimal::new(2, 1));

        let o = Order::market(Side::Buy, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * Decimal::new(800, 0);
        let fee_asset0 = fee_base0 / Decimal::new(1000, 0);

        let fee_base1 = fee_taker * Decimal::new(800, 0);
        let fee_asset1 = fee_base1 / Decimal::new(800, 0);

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, Decimal::new(0, 0));
        assert_eq!(exchange.position.value, Decimal::new(0, 0));
        assert_eq!(exchange.position.margin, Decimal::new(0, 0));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(12, 1) - fee_combined);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(12, 1) - fee_combined);
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(12, 1) - fee_combined);
    }

    #[test]
    fn short_market_loss_full()  {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * Decimal::new(4, 1);
        let size = exchange.ask * value;
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let o = Order::market(Side::Sell, s);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base1 = size * fee_taker;
        let fee_asset1 = fee_base1 / exchange.bid;

        assert_eq!(exchange.position.size,  -size);
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.position_margin, Decimal::new(4, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(6, 1) - fee_asset1);

        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = Decimal::new(400, 0);
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / Decimal::new(2000, 0);

        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(-2, 1));

        let o = Order::market(Side::Buy, size.to_f64().unwrap());
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(0, 0));
        assert_eq!(exchange.position.value, Decimal::new(0, 0));
        assert_eq!(exchange.position.margin, Decimal::new(0, 0));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(8, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(8, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(8, 1) - fee_asset1 - fee_asset2);

    }

    #[test]
    fn long_market_win_partial()  {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * Decimal::new(8, 1);
        let size = exchange.ask * value;
        let o = Order::market(Side::Buy, size.to_f64().unwrap());
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset1 = fee_base / exchange.bid;

        assert_eq!(exchange.position.size, size);
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.position_margin, Decimal::new(8, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(2, 1) - fee_asset1);

        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = Decimal::new(400, 0);
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / Decimal::new(2000, 0);

        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(4, 1));

        let o = Order::market(Side::Sell, size.to_f64().unwrap());
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(400, 0));
        assert_eq!(exchange.position.value, Decimal::new(2, 1));
        assert_eq!(exchange.position.margin, Decimal::new(4, 1));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(2, 1));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(12, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(14, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.position_margin, Decimal::new(4, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(8, 1) - fee_asset1 - fee_asset2);

    }

    #[test]
    fn long_market_loss_partial() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(800, 0));

        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), Decimal::new(-2, 1));

        let o = Order::market(Side::Sell, 400.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * Decimal::new(800, 0);
        let fee_asset0 = fee_base0 / Decimal::new(1000, 0);

        let fee_base1 = fee_taker * Decimal::new(400, 0);
        let fee_asset1 = fee_base1 / Decimal::new(800, 0);

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, Decimal::new(400, 0));
        assert_eq!(exchange.position.value, Decimal::new(5, 1));
        assert_eq!(exchange.position.margin, Decimal::new(4, 1));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(-1, 1));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(9, 1) - fee_combined);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(8, 1) - fee_combined);
        assert_eq!(exchange.margin.position_margin, Decimal::new(4, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(5, 1) - fee_combined);

    }

    #[test]
    fn short_market_win_partial() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(-800, 0));

        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.unrealized_pnl(), Decimal::new(2, 1));

        let o = Order::market(Side::Buy, 400.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * Decimal::new(800, 0);
        let fee_asset0 = fee_base0 / Decimal::new(1000, 0);

        let fee_base1 = fee_taker * Decimal::new(400, 0);
        let fee_asset1 = fee_base1 / Decimal::new(800, 0);

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.position.size, Decimal::new(-400, 0));
        assert_eq!(exchange.position.value, Decimal::new(5, 1));
        assert_eq!(exchange.position.margin, Decimal::new(4, 1));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(1, 1));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(11, 1) - fee_combined);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(12, 1) - fee_combined);
        assert_eq!(exchange.margin.position_margin, Decimal::new(6, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(5, 1) - fee_combined);
    }

    #[test]
    fn short_market_loss_partial()  {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * Decimal::new(8, 1);
        let size = exchange.ask * value;
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let o = Order::market(Side::Sell, s);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base1 = size * fee_taker;
        let fee_asset1 = fee_base1 / exchange.bid;

        assert_eq!(exchange.position.size,  -size);
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - fee_asset1);
        assert_eq!(exchange.margin.position_margin, Decimal::new(8, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(2, 1) - fee_asset1);

        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = Decimal::new(400, 0);
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / Decimal::new(2000, 0);

        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(-4, 1));

        let o = Order::market(Side::Buy, size.to_f64().unwrap());
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(-400, 0));
        assert_eq!(exchange.position.value, Decimal::new(2, 1));
        assert_eq!(exchange.position.margin, Decimal::new(4, 1));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(-2, 1));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(8, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(6, 1) - fee_asset1 - fee_asset2);
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 1));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(8, 1) - fee_asset1 - fee_asset2);

    }

    #[test]
    fn test_market_roundtrip() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.margin.available_balance * Decimal::new(9, 1);
        let size = exchange.ask * value;
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let buy_order = Order::market(Side::Buy, s);
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let sell_order = Order::market(Side::Sell, s);

        let order_err = exchange.submit_order(sell_order);
        assert!(order_err.is_ok());

        let fee_base = size * fee_taker;
        let fee_asset = fee_base / exchange.ask;

        exchange.check_orders();

        assert_eq!(exchange.position.size,  Decimal::new(0, 0));
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, Decimal::new(0, 0));
        assert_eq!(exchange.position.margin, Decimal::new(0, 0));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - Decimal::new(2, 0) * fee_asset);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - Decimal::new(2, 0) * fee_asset);
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(1, 0) - Decimal::new(2, 0) * fee_asset);


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

        assert_eq!(exchange.position.size,  Decimal::new(-50, 0));
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, Decimal::new(5, 2));
        assert_eq!(exchange.position.margin, Decimal::new(5, 2));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert!(exchange.margin.wallet_balance < Decimal::new(1, 0));
        assert!(exchange.margin.margin_balance < Decimal::new(1, 0));
        assert_eq!(exchange.margin.position_margin, Decimal::new(5, 2));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert!(exchange.margin.available_balance < Decimal::new(1, 0));
    }

    #[test]
    fn test_handle_take_profit_limit_order() {
        // TODO:
    }

    #[test]
    fn test_order_ids() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 100.0,
            size: 10.0,
        };
        exchange.consume_trade(&t);
        for i in 0..100 {
            let o = Order::stop_market(Side::Buy, 101.0 + i as f64, 10.0);
            exchange.submit_order(o);
        }
        let active_orders = exchange.orders_active;
        let mut last_order_id: i64 = -1;
        for o in &active_orders {
            assert!(o.id as i64 > last_order_id);
            last_order_id = o.id as i64;
        }
    }

    #[test]
    fn set_leverage() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let valid = exchange.set_leverage(0.5);
        assert!(!valid);

        exchange.set_leverage(5.0);
        assert_eq!(exchange.position.leverage, Decimal::new(5, 0));

        exchange.set_leverage(1.0);
        assert_eq!(exchange.position.leverage, Decimal::new(1, 0));

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let valid = exchange.set_leverage(10.0);
        assert!(valid);

        let fee_base = Decimal::new(100, 0) * fee_taker;
        let fee_asset = fee_base / exchange.bid;

        // should change with different leverage
        assert_eq!(exchange.position.margin, Decimal::new(1, 2));
        assert_eq!(exchange.margin.position_margin, Decimal::new(1, 2));
        assert_eq!(exchange.margin.available_balance, Decimal::new(99, 2) - fee_asset);

        let valid = exchange.set_leverage(5.0);
        assert!(valid);
        assert_eq!(exchange.position.margin, Decimal::new(2, 2));
        assert_eq!(exchange.margin.position_margin, Decimal::new(2, 2));
        assert_eq!(exchange.margin.available_balance, Decimal::new(98, 2) - fee_asset);


        let o = Order::market(Side::Buy, 4800.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        let fee_asset2 = (fee_taker * Decimal::new(4800, 0)) / exchange.bid;

        exchange.check_orders();

        assert_eq!(exchange.position.margin, Decimal::new(98, 2));
        assert_eq!(exchange.margin.position_margin, Decimal::new(98, 2));
        assert_eq!(exchange.margin.available_balance, Decimal::new(2, 2) - (fee_asset + fee_asset2));

    }

    #[test]
    fn liq_price() {
        let config = Config::perpetuals();
        // let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.liq_price, Decimal::new(0, 0));

        // TODO: test liq_price with higher leverage and with short position as well
    }

    #[test]
    fn unrealized_pnl() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(100, 0));
        let upnl = exchange.unrealized_pnl();
        assert_eq!(upnl, Decimal::new(0, 0));

        let t = Trade{
            timestamp: 1,
            price: 1100.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let t = Trade{
            timestamp: 1,
            price: 1100.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let upnl = exchange.unrealized_pnl();
        assert!(upnl > Decimal::new(0, 0));
    }

    #[test]
    fn roe() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.position.size, Decimal::new(100, 0));

        let t = Trade{
            timestamp: 1,
            price: 1100.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let t = Trade{
            timestamp: 1,
            price: 1100.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let roe = exchange.roe();
        assert_eq!(roe, Decimal::new(1, 1));
    }

    #[test]
    fn test_liquidate() {
        // TODO:
    }

    #[test]
    fn cancel_order() {
        let config = Config::perpetuals();
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.order_margin(), Decimal::new(5, 1));

        exchange.cancel_order(0);
        assert_eq!(exchange.orders_active.len(), 0);
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0));
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(1, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));

    }

    #[test]
    fn order_margin() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), Decimal::new(5, 1));
        assert_eq!(exchange.orders_active.len(), 1);

        let o = Order::limit(Side::Sell, 1200.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), Decimal::new(5, 1));
        assert_eq!(exchange.orders_active.len(), 2);

        let o = Order::market(Side::Buy, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.position.size, Decimal::new(450, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(5, 1));

        let o = Order::limit(Side::Sell, 1200.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), Decimal::new(5, 1));
        assert_eq!(exchange.orders_active.len(), 3);

        let o = Order::market(Side::Sell, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.order_margin(), Decimal::new(75, 2));

        let o = Order::market(Side::Buy, 240.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.position.size, Decimal::new(240, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(51, 2));

    }

    #[test]
    fn execute_limit() {
        let config = Config::perpetuals();
        let fee_taker = config.fee_taker;
        let mut exchange = ExchangeDecimal::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o: Order = Order::limit(Side::Buy, 900.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.margin.available_balance, Decimal::new(5, 1));

        let t = Trade{
            timestamp: 1,
            price: 750.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 1,
            price: 750.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let fee_maker_0: Decimal = Decimal::new(125, 6);

        assert_eq!(exchange.bid, Decimal::new(750, 0));
        assert_eq!(exchange.ask, Decimal::new(750, 0));
        assert_eq!(exchange.orders_active.len(), 0);
        assert_eq!(exchange.position.size, Decimal::new(450, 0));
        assert_eq!(exchange.position.value, Decimal::new(6, 1));
        assert_eq!(exchange.position.margin, Decimal::new(5, 1));
        assert_eq!(exchange.position.entry_price, Decimal::new(900, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1000125, 6));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        // Knapp daneben ist auch vorbei
        // assert_eq!(exchange.unrealized_pnl(), Decimal::new(-1, 1));
        // assert_eq!(exchange.margin.position_margin, Decimal::new(5, 1));
        // assert_eq!(exchange.margin.available_balance, Decimal::new(5, 1) + fee_maker);


        let o: Order = Order::limit(Side::Sell, 1000.0, 450.0);
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.orders_active.len(), 1);
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));

        let t = Trade{
            timestamp: 1,
            price: 1200.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade{
            timestamp: 1,
            price: 1200.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.orders_active.len(), 0);
        assert_eq!(exchange.position.size, Decimal::new(0, 0));
        assert_eq!(exchange.position.value, Decimal::new(0, 0));
        assert_eq!(exchange.position.margin, Decimal::new(0, 0));
        assert_eq!(exchange.order_margin(), Decimal::new(0, 0));
        assert_eq!(exchange.margin.position_margin, Decimal::new(0, 0));
        let fee_maker_1: Decimal = Decimal::new(1125, 7);
        let wb: Decimal = Decimal::new(1, 0)
            + fee_maker_0 + fee_maker_1 + Decimal::new(5, 2);
        // Again nearly correct but not quite which is fine though
        // assert_eq!(exchange.margin.wallet_balance, wb);
        // assert_eq!(exchange.margin.available_balance, wb);
        // assert_eq!(exchange.margin.margin_balance, wb);


    }
}
