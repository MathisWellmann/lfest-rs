extern crate trade_aggregation;

use trade_aggregation::common::*;
use crate::orders_float::*;
use crate::orders_decimal::{OrderType, OrderError, Side};
use crate::config_float::*;
use chrono::prelude::*;
use crate::exchange_decimal::FeeType;


#[derive(Debug, Clone)]
pub struct ExchangeFloat {
    pub config: Config,
    pub position: PositionFloat,
    pub margin: MarginFloat,
    pub acc_tracker: AccTrackerFloat,
    pub bid: f64,
    pub ask: f64,
    init: bool,
    pub total_rpnl: f64,
    pub rpnls: Vec<f64>,
    orders_done: Vec<OrderFloat>,
    orders_executed: Vec<OrderFloat>,
    pub orders_active: Vec<OrderFloat>,
    next_order_id: u64,
}

#[derive(Debug, Clone)]
pub struct MarginFloat {
    pub wallet_balance: f64,
    pub margin_balance: f64,
    position_margin: f64,
    order_margin: f64,
    pub available_balance: f64,
}

#[derive(Debug, Clone)]
pub struct PositionFloat {
    pub size: f64,
    value: f64,
    pub entry_price: f64,
    liq_price: f64,
    margin: f64,
    pub leverage: f64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone)]
pub struct AccTrackerFloat {
    pub num_trades: i64,
    pub num_buys: i64,
}

impl ExchangeFloat {

    pub fn new(config: Config) -> ExchangeFloat {
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
            acc_tracker: AccTrackerFloat{
                num_trades: 0,
                num_buys: 0,
            },
            total_rpnl: 0.0,
            bid: 0.0,
            ask: 0.0,
            init: true,
            rpnls: Vec::new(),
            orders_done: Vec::new(),
            orders_executed: Vec::new(),
            orders_active: Vec::new(),
            next_order_id: 0,
        }
    }

    // sets the new leverage of position
    // returns true if successful
    pub fn set_leverage(&mut self, l: f64) -> bool {
        if l > self.config.max_leverage {
            return false
        } else if l < self.config.min_leverage {
            return false
        }

        let new_position_margin = (self.position.value / l) + self.position.unrealized_pnl;
        if new_position_margin > self.margin.wallet_balance {
            return false
        }
        self.position.leverage = l;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.unrealized_pnl();
        self.margin.available_balance = self.margin.margin_balance - self.margin.order_margin - self.margin.position_margin;
        self.position.margin = (self.position.value / self.position.leverage);

        return true
    }

    // consume_candle update the exchange state with th new candle.
    // returns true if position has been liquidated
    pub fn consume_trade(&mut self, trade: &Trade) -> bool {
        let price = trade.price;
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

        self.update_position_stats();
        self.check_orders();

        return false
    }

    // consume_candle update the bid and ask price given a candle using its close price
    // returns true if position has been liquidated
    pub fn consume_candle(&mut self, candle: &Candle) -> bool {
        self.bid = candle.close;
        self.ask = candle.close;

        if self.check_liquidation() {
            return true
        }

        self.update_position_stats();
        self.check_orders();
        return false
    }

    // candle an active order
    // returns true if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<OrderFloat> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.orders_active.remove(i);
                let margin = old_order.size / old_order.price / self.position.leverage;
                self.margin.order_margin -= margin;
                self.margin.available_balance += margin;
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

    pub fn submit_order(&mut self, mut o: &OrderFloat) -> Option<OrderError> {
        if self.orders_active.len() >= self.config.max_active_orders {
            return Some(OrderError::MaxActiveOrders)
        }
        if o.size <= 0.0 {
            return Some(OrderError::InvalidOrder)
        }
        let order_err: Option<OrderError> = match o.order_type {
            OrderType::Market => self.validate_market_order(o),
            OrderType::Limit => self.validate_limit_order(o),
            OrderType::StopMarket => self.validate_stop_market_order(o),
            OrderType::TakeProfitLimit => self.validate_take_profit_limit_order(o),
            OrderType::TakeProfitMarket => self.validate_take_profit_market_order(o),
        };
        if order_err.is_some() {
            return order_err
        }

        let mut o = o.clone();

        // assign unique order id
        o.id = self.next_order_id;
        self.next_order_id += 1;

        // assign timestamp
        let now = Utc::now();
        o.timestamp = now.timestamp_millis() as u64;

        match o.order_type {
            OrderType::Market => {
                // immediately execute market order
                self.execute_market(o.side, o.size);
                return None
            }
            _ => {},
        }
        self.orders_active.push(o);

        return None
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

    pub fn ammend_order(&mut self, order_id: u64, new_order: OrderFloat) -> Option<OrderError> {
        // TODO:
        return None
    }

    fn check_liquidation(&mut self) -> bool {
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

    fn deduce_fees(&mut self, t: FeeType, side: Side, amount_base: f64) {
        let price = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
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
        self.deduce_fees(FeeType::Taker, side, amount_base);

        let price: f64 = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let old_position_size = self.position.size;
        let old_entry_price: f64 =  if self.position.size == 0.0 {
            price
        } else {
            self.position.entry_price
        };

        match side {
            Side::Buy => {
                if self.position.size < 0.0 {
                    if amount_base >= self.position.size.abs() {
                        // realize_pnl
                        let rpnl = self.position.size.abs() * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.total_rpnl += rpnl;
                        self.rpnls.push(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size += amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;

                    } else {
                        // realize pnl
                        let rpnl = amount_base * ((1.0 / price) - (1.0 / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        self.total_rpnl += rpnl;
                        self.rpnls.push(rpnl);

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
                        self.total_rpnl += rpnl;
                        self.rpnls.push(rpnl);

                        let size_diff = amount_base - self.position.size.abs();
                        self.position.size -= amount_base;
                        self.position.margin = size_diff / price / self.position.leverage;
                        self.position.entry_price = price;

                    } else {
                        // realize pnl
                        let rpnl = amount_base * ((1.0 / self.position.entry_price) - (1.0 / price));
                        self.margin.wallet_balance += rpnl;
                        self.total_rpnl += rpnl;
                        self.rpnls.push(rpnl);

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

    fn liquidate(&mut self) {
        // TODO: better liquidate function
        if self.position.size > 0.0 {
            let rpnl: f64 = self.position.size.abs() * (1.0 / self.position.entry_price - 1.0 / self.bid);
            self.total_rpnl += rpnl;
            self.rpnls.push(rpnl);
            self.margin.wallet_balance += rpnl;

            self.position.margin = 0.0;

            self.position.entry_price = 0.0;

            self.position.size = 0.0;
            self.position.value = 0.0;

            self.position.unrealized_pnl = 0.0;

            self.update_liq_price();

            self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
            self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;

            self.acc_tracker.num_trades += 1;

        } else {
            let rpnl = self.position.size.abs() * (1.0 / self.position.entry_price - 1.0 / self.ask);
            self.total_rpnl += rpnl;
            self.rpnls.push(rpnl);
            self.margin.wallet_balance += rpnl;

            self.position.margin = 0.0;

            self.position.entry_price = 0.0;

            self.position.size = 0.0;
            self.position.value = 0.0;

            self.position.unrealized_pnl = 0.0;

            self.update_liq_price();

            self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
            self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;

            self.acc_tracker.num_buys += 1;
            self.acc_tracker.num_trades += 1;
        }

        self.update_position_stats();
    }

    fn check_orders(&mut self) {
        for i in 0..self.orders_active.len() {
            match self.orders_active[i].order_type {
                OrderType::Market => self.handle_market_order(i),
                OrderType::Limit => self.handle_limit_order(i),
                OrderType::StopMarket => self.handle_stop_market_order(i),
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
                self.acc_tracker.num_trades += 1;
                match self.orders_active[i].side {
                    Side::Buy => self.acc_tracker.num_buys += 1,
                    Side::Sell => {},
                }
                let exec_order = self.orders_active.remove(i);
                self.orders_executed.push(exec_order);
            }
            i += 1;
        }
    }

    fn handle_stop_market_order(&mut self, order_index: usize) {
        let o: &OrderFloat = &self.orders_active[order_index];
        match o.side {
            Side::Buy => {
                if o.price > self.ask { return }
                self.execute_market(Side::Buy, o.size);
                self.orders_active[order_index].mark_done();
            },
            Side::Sell => {
                if o.price > self.bid { return }
                self.execute_market(Side::Sell, o.size);
                self.orders_active[order_index].mark_done();
            },
        }
    }

    fn handle_take_profit_market_order(&mut self, order_index: usize) {
        let o: &OrderFloat = &self.orders_active[order_index];
        match o.side {
            Side::Buy => { if o.price < self.bid { return }
                self.execute_market(Side::Buy, o.size * self.ask);
                self.orders_active[order_index].mark_done();
            },
            Side::Sell => { if o.price > self.ask { return }
                self.execute_market(Side::Sell, o.size * self.bid);
                self.orders_active[order_index].mark_done();
            },
        }
    }

    fn handle_market_order(&mut self, order_index: usize) {
        let o: &OrderFloat = &self.orders_active[order_index];
        match o.side {
            Side::Buy => self.execute_market(Side::Buy, o.size),
            Side::Sell => self.execute_market(Side:: Sell, o.size),
        }
        self.orders_active[order_index].mark_done();
    }

    fn handle_limit_order(&mut self, order_index: usize) {
        // TODO:
    }

    fn handle_take_profit_limit_order(&mut self, order_index: usize) {
        // TODO:
    }

    // check if market order is correct
    fn validate_market_order(&mut self, o: &OrderFloat) -> Option<OrderError> {
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

    fn validate_limit_order(&self, o: &OrderFloat) -> Option<OrderError> {
        // TODO:
        return None
    }

    fn validate_take_profit_limit_order(&self, o: &OrderFloat) -> Option<OrderError> {
        // TODO:
        return None
    }

    // returns true if order is valid
    fn validate_stop_market_order(&mut self, o: &OrderFloat) -> Option<OrderError> {
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

    fn validate_take_profit_market_order(&self, o: &OrderFloat) -> Option<OrderError> {
        return match o.side {
            Side::Buy => { if o.price > self.bid { return Some(OrderError::InvalidOrder) }
                None
            },
            Side::Sell => { if o.price < self.ask { return Some(OrderError::InvalidOrder) }
                None
            },
        }
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

    fn roe(&self) -> f64 {
        return if self.position.size > 0.0 {
            (self.bid - self.position.entry_price) / self.position.entry_price
        } else {
            (self.position.entry_price - self.ask) / self.position.entry_price
        }
    }

}

pub fn min(val0: f64, val1: f64) -> f64 {
    if val0 < val1 {
        return val0
    }
    return val1
}

