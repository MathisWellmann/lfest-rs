extern crate trade_aggregation;

use trade_aggregation::common::*;
use crate::orders::*;
use crate::config::*;
use chrono::prelude::*;
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};


#[derive(Debug, Clone)]
pub struct Exchange {
    pub config: Config,
    pub position: Position,
    pub margin: Margin,
    pub acc_tracker: AccTracker,
    pub bid: Decimal,
    pub ask: Decimal,
    init: bool,
    pub total_rpnl: f64,
    pub rpnls: Vec<f64>,
    orders_done: Vec<Order>,
    orders_executed: Vec<Order>,
    pub orders_active: Vec<Order>,
    next_order_id: u64,
}

#[derive(Debug, Clone)]
pub struct Margin {
    pub wallet_balance: Decimal,
    pub margin_balance: Decimal,
    position_margin: Decimal,
    order_margin: Decimal,
    pub available_balance: Decimal,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub size: Decimal,
    value: Decimal,
    pub entry_price: Decimal,
    liq_price: Decimal,
    margin: Decimal,
    pub leverage: Decimal,
    pub unrealized_pnl: Decimal,
}

#[derive(Debug, Clone)]
pub struct AccTracker {
    pub num_trades: i64,
    pub num_buys: i64,
}

#[derive(Debug, Clone)]
pub enum FeeType {
    Maker,
    Taker,
}

impl Exchange {

    pub fn new(config: Config) -> Exchange {
        return Exchange{
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
                order_margin: Decimal::new(0, 0),
                available_balance: Decimal::new(1, 0),
            },
            acc_tracker: AccTracker{
                num_trades: 0,
                num_buys: 0,
            },
            total_rpnl: 0.0,
            bid: Decimal::new(0, 0),
            ask: Decimal::new(0, 0),
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
        let l = Decimal::from_f64(l).unwrap();
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

        self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
        self.margin.available_balance = self.margin.margin_balance - self.margin.position_margin;

        self.check_orders();

        return false
    }

    // consume_candle update the bid and ask price given a candle using its close price
    // returns true if position has been liquidated
    pub fn consume_candle(&mut self, candle: &Candle) -> bool {
        self.bid = Decimal::from_f64(candle.close).unwrap();
        self.ask = Decimal::from_f64(candle.close).unwrap();

        if self.check_liquidation() {
            return true
        }

        self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;
        self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
        self.margin.available_balance = self.margin.margin_balance - self.margin.position_margin;

        self.check_orders();
        return false
    }

    // candle an active order
    // returns true if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<Order> {
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

    pub fn query_active_orders(&self, order_id: u64) -> Option<&Order> {
        for (i, o) in self.orders_active.iter().enumerate() {
            if o.id == order_id {
                return self.orders_active.get(i);
            }
        }
        None
    }

    pub fn submit_order(&mut self, mut o: &Order) -> Option<OrderError> {
        if self.orders_active.len() >= self.config.max_active_orders {
            return Some(OrderError::MaxActiveOrders)
        }
        if o.size <= Decimal::new(0, 0) {
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

        self.orders_active.push(o);

        return None
    }

    pub fn unrealized_pnl(&self) -> Decimal {
        if self.position.size == Decimal::new(0, 0) {
            return Decimal::new(0, 0);
        } else if self.position.size > Decimal::new(0, 0) {
            return ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / self.bid)) * self.position.size.abs();
        } else {
            return ((Decimal::new(1, 0) / self.ask) - (Decimal::new(1, 0) / self.position.entry_price)) * self.position.size.abs();
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

    pub fn ammend_order(&mut self, order_id: u64, new_order: Order) -> Option<OrderError> {
        // TODO:
        return None
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

    fn deduce_fees(&mut self, t: FeeType, side: Side, amount_base: Decimal) {
        let price = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
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
        self.position.unrealized_pnl = self.unrealized_pnl();
        let position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
        self.position.margin = position_margin;
        self.margin.position_margin = position_margin;
        self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;
        self.margin.available_balance = self.margin.wallet_balance - self.margin.position_margin - self.margin.order_margin;
    }

    fn execute_market(&mut self, side: Side, amount_base: Decimal) {
        self.deduce_fees(FeeType::Taker, side, amount_base);

        let price: Decimal = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let old_position_size = self.position.size;

        match side {
            Side::Buy => {
                if self.position.size < Decimal::new(0,0) {
                    if amount_base > self.position.size {
                        // realize_pnl
                        let rpnl = self.position.size * ((Decimal::new(1, 0) / price) - (Decimal::new(1, 0) / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.total_rpnl += rpnl_f64;
                        self.rpnls.push(rpnl_f64);

                        let size_diff = amount_base - self.position.size;
                        self.position.size += amount_base;
                        self.position.value = size_diff / price;

                    } else {
                        self.position.size += amount_base;
                        self.position.value -= amount_base / price;
                        // realize pnl
                        let rpnl = amount_base * ((Decimal::new(1, 0) / price) - (Decimal::new(1, 0) / self.position.entry_price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.total_rpnl += rpnl_f64;
                        self.rpnls.push(rpnl_f64);
                    }
                } else {
                    self.position.size += amount_base;
                    self.position.value += amount_base / price;
                }
            },
            Side::Sell => {
                if self.position.size > Decimal::new(0, 0) {
                    if amount_base > self.position.size {
                        // realize pnl
                        let rpnl = self.position.size * ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.total_rpnl += rpnl_f64;
                        self.rpnls.push(rpnl_f64);

                        let size_diff = amount_base - self.position.size;
                        self.position.size -= amount_base;
                        self.position.value = size_diff / price;

                    } else {
                        self.position.size -= amount_base;
                        self.position.value -= amount_base / price;
                        // realize pnl
                        let rpnl = amount_base * ((Decimal::new(1, 0) / self.position.entry_price) - (Decimal::new(1, 0) / price));
                        self.margin.wallet_balance += rpnl;
                        let rpnl_f64 = rpnl.to_f64().unwrap();
                        self.total_rpnl += rpnl_f64;
                        self.rpnls.push(rpnl_f64);
                    }
                } else {
                    self.position.size -= amount_base;
                    self.position.value += amount_base / price;
                }
            },
        }

        self.position.entry_price = ((price * amount_base) + self.position.entry_price * old_position_size.abs())
            / (amount_base + old_position_size.abs());

        self.update_position_stats();
        self.update_liq_price();
    }

    fn liquidate(&mut self) {
        // TODO: better liquidate function
        if self.position.size > Decimal::new(0, 0) {
            let rpnl: Decimal = self.position.size.abs() * (Decimal::new(1, 0) / self.position.entry_price - Decimal::new(1, 0) / self.bid);
            let rpnl_f64 = rpnl.to_f64().unwrap();
            self.total_rpnl += rpnl_f64;
            self.rpnls.push(rpnl_f64);
            self.margin.wallet_balance += rpnl;

            self.position.margin = Decimal::new(0, 0);

            self.position.entry_price = Decimal::new(0, 0);

            self.position.size = Decimal::new(0, 0);
            self.position.value = Decimal::new(0, 0);

            self.position.unrealized_pnl = Decimal::new(0, 0);

            self.update_liq_price();

            self.margin.position_margin = (self.position.value / self.position.leverage) + self.position.unrealized_pnl;
            self.margin.margin_balance = self.margin.wallet_balance + self.position.unrealized_pnl;

            self.acc_tracker.num_trades += 1;

        } else {
            let rpnl = self.position.size.abs() * (Decimal::new(1, 0) / self.position.entry_price - Decimal::new(1, 0) / self.ask);
            let rpnl_f64 = rpnl.to_f64().unwrap();
            self.total_rpnl += rpnl_f64;
            self.rpnls.push(rpnl_f64);
            self.margin.wallet_balance += rpnl;

            self.position.margin = Decimal::new(0, 0);

            self.position.entry_price = Decimal::new(0, 0);

            self.position.size = Decimal::new(0, 0);
            self.position.value = Decimal::new(0, 0);

            self.position.unrealized_pnl = Decimal::new(0, 0);

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
        let o: &Order = &self.orders_active[order_index];
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
        let o: &Order = &self.orders_active[order_index];
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
        let o: &Order = &self.orders_active[order_index];
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
    fn validate_market_order(&mut self, o: &Order) -> Option<OrderError> {
        let price = match o.side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };
        let fee_base = self.config.fee_taker * o.size;
        let fee_asset = fee_base / self.bid;

        // check if enough available balance for initial margin requirements
        let order_margin: Decimal = o.size / price / self.position.leverage;
        let order_err: Option<OrderError> = match o.side {
            Side::Buy => {
                if self.position.size > Decimal::new(0, 0) {
                    if order_margin + fee_asset > self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                } else {
                    if order_margin + fee_asset > self.margin.position_margin + self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
                    }
                    None
                }
            },
            Side::Sell => {
                if self.position.size > Decimal::new(0, 0) {
                    if order_margin + fee_asset > self.margin.position_margin + self.margin.available_balance {
                        return Some(OrderError::NotEnoughAvailableBalance)
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
        if order_err.is_some() {
            return order_err
        }

        return None
    }

    fn validate_limit_order(&self, o: &Order) -> Option<OrderError> {
        // TODO:
        return None
    }

    fn validate_take_profit_limit_order(&self, o: &Order) -> Option<OrderError> {
        // TODO:
        return None
    }

    // returns true if order is valid
    fn validate_stop_market_order(&mut self, o: &Order) -> Option<OrderError> {
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

    fn validate_take_profit_market_order(&self, o: &Order) -> Option<OrderError> {
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
        if self.position.size == Decimal::new(0, 0) {
            self.position.liq_price = Decimal::new(0, 0);
        } else if self.position.size > Decimal::new(0, 0) {
            self.position.liq_price = self.position.entry_price - (self.position.entry_price / self.position.leverage);
        } else {
            self.position.liq_price = self.position.entry_price + (self.position.entry_price / self.position.leverage);
        }
    }

    fn roe(&self) -> Decimal {
        return if self.position.size > Decimal::new(0, 0) {
            (self.bid - self.position.entry_price) / self.position.entry_price
        } else {
            (self.position.entry_price - self.ask) / self.position.entry_price
        }
    }

}

pub fn min(val0: Decimal, val1: Decimal) -> Decimal {
    if val0 < val1 {
        return val0
    }
    return val1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_market_order() {
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let size = exchange.ask * exchange.margin.available_balance * Decimal::new(4, 1);
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let o = Order::market(Side::Buy, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        let o = Order::market(Side::Sell, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_none());

        let size = exchange.ask * exchange.margin.available_balance * Decimal::new(2, 0);
        let s = size.to_string();
        let s = s.parse::<f64>().unwrap();
        let o = Order::market(Side::Buy, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());

        let o = Order::market(Side::Sell, s);
        let order_err = exchange.validate_market_order(&o);
        assert!(order_err.is_some());
    }

    #[test]
    fn test_validate_limit_order() {
        // TODO
    }

    #[test]
    fn test_validate_stop_market_order() {
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
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
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
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
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 100.0);
        let valid = exchange.submit_order(&o);
        match valid {
            Some(_) => panic!("order not valid!"),
            None => {},
        }
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
    fn test_buy_market()  {
        let config = Config::xbt_usd();
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
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
        let o = Order::market(Side::Buy, s);
        let order_err = exchange.submit_order(&o);
        match order_err {
            Some(OrderError::InvalidOrder) => panic!("invalid order"),
            Some(OrderError::MaxActiveOrders) => panic!("max_active_orders"),
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidTriggerPrice) => panic!("invalid trigger price"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            None => {},
        }

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset = fee_base / exchange.bid;

        assert_eq!(exchange.position.size, size);
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - fee_asset);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - fee_asset);
        assert_eq!(exchange.margin.position_margin, Decimal::new(4, 1));
        assert_eq!(exchange.margin.order_margin, Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(6, 1) - fee_asset);
    }

    #[test]
    fn test_sell_market()  {
        let config = Config::xbt_usd();
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
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
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset = fee_base / exchange.bid;

        assert_eq!(exchange.position.size,  -size);
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, value);
        assert_eq!(exchange.position.margin, value);
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert_eq!(exchange.margin.wallet_balance, Decimal::new(1, 0) - fee_asset);
        assert_eq!(exchange.margin.margin_balance, Decimal::new(1, 0) - fee_asset);
        assert_eq!(exchange.margin.position_margin, Decimal::new(4, 1));
        assert_eq!(exchange.margin.order_margin, Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(6, 1) - fee_asset);
    }

    #[test]
    fn test_market_roundtrip() {
        let config = Config::xbt_usd();
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
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
        let order_err = exchange.submit_order(&buy_order);
        match order_err {
            Some(OrderError::InvalidOrder) => panic!("invalid order"),
            Some(OrderError::MaxActiveOrders) => panic!("max_active_orders"),
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidTriggerPrice) => panic!("invalid trigger price"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            None => {},
        }
        exchange.check_orders();

        let sell_order = Order::market(Side::Sell, s);

        let order_err = exchange.submit_order(&sell_order);
        match order_err {
            Some(OrderError::InvalidOrder) => panic!("invalid order"),
            Some(OrderError::MaxActiveOrders) => panic!("max_active_orders"),
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidTriggerPrice) => panic!("invalid trigger price"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            None => {},
        }

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
        assert_eq!(exchange.margin.order_margin, Decimal::new(0, 0));
        assert_eq!(exchange.margin.available_balance, Decimal::new(1, 0) - Decimal::new(2, 0) * fee_asset);


        let size = 900.0;
        let buy_order = Order::market(Side::Buy, size);
        let order_err = exchange.submit_order(&buy_order);
        match order_err {
            Some(OrderError::InvalidOrder) => panic!("invalid order"),
            Some(OrderError::MaxActiveOrders) => panic!("max_active_orders"),
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidTriggerPrice) => panic!("invalid trigger price"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            None => {},
        }
        exchange.check_orders();

        let size = 950.0;
        let sell_order = Order::market(Side::Sell, size);

        let order_err = exchange.submit_order(&sell_order);
        match order_err {
            Some(OrderError::InvalidOrder) => panic!("invalid order"),
            Some(OrderError::MaxActiveOrders) => panic!("max_active_orders"),
            Some(OrderError::NotEnoughAvailableBalance) => panic!("not enough available balance"),
            Some(OrderError::InvalidTriggerPrice) => panic!("invalid trigger price"),
            Some(OrderError::InvalidOrderSize) => panic!("invalid order size"),
            None => {},
        }

        exchange.check_orders();

        assert_eq!(exchange.position.size,  Decimal::new(-50, 0));
        assert_eq!(exchange.position.entry_price, Decimal::new(1000, 0));
        assert_eq!(exchange.position.value, Decimal::new(5, 2));
        assert_eq!(exchange.position.margin, Decimal::new(5, 2));
        assert_eq!(exchange.position.unrealized_pnl, Decimal::new(0, 0));
        assert!(exchange.margin.wallet_balance < Decimal::new(1, 0));
        assert!(exchange.margin.margin_balance < Decimal::new(1, 0));
        assert_eq!(exchange.margin.position_margin, Decimal::new(5, 2));
        assert_eq!(exchange.margin.order_margin, Decimal::new(0, 0));
        assert!(exchange.margin.available_balance < Decimal::new(1, 0));
    }

    #[test]
    fn test_handle_take_profit_limit_order() {
        // TODO:
    }

    #[test]
    fn test_order_ids() {
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 100.0,
            size: 10.0,
        };
        exchange.consume_trade(&t);
        for i in 0..100 {
            let o = Order::stop_market(Side::Buy, 101.0 + i as f64, 10.0);
            exchange.submit_order(&o);
        }
        let active_orders = exchange.orders_active;
        let mut last_order_id: i64 = -1;
        for o in &active_orders {
            assert!(o.id as i64 > last_order_id);
            last_order_id = o.id as i64;
        }
    }

    #[test]
    fn test_order_margin() {
        // let config = Config::xbt_usd();
        // let mut exchange = new(config);
        // let t = Trade {
        //     timestamp: 0,
        //     price: 100.0,
        //     size: 100.0,
        // };
        // exchange.consume_trade(&t);

    }

    #[test]
    fn set_leverage() {
        let config = Config::xbt_usd();
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let valid = exchange.set_leverage(101.0);
        assert!(!valid);
        let valid = exchange.set_leverage(0.5);
        assert!(!valid);

        exchange.set_leverage(5.0);
        assert_eq!(exchange.position.leverage, Decimal::new(5, 0));

        exchange.set_leverage(1.0);
        assert_eq!(exchange.position.leverage, Decimal::new(1, 0));

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());

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
        assert_eq!(exchange.position.margin, Decimal::new(2, 2));
        assert_eq!(exchange.margin.position_margin, Decimal::new(2, 2));
        assert_eq!(exchange.margin.available_balance, Decimal::new(98, 2) - fee_asset);


        let o = Order::market(Side::Buy, 4800.0);
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());

        let fee_asset2 = (fee_taker * Decimal::new(4800, 0)) / exchange.bid;

        exchange.check_orders();

        assert_eq!(exchange.position.margin, Decimal::new(98, 2));
        assert_eq!(exchange.margin.position_margin, Decimal::new(98, 2));
        assert_eq!(exchange.margin.available_balance, Decimal::new(2, 2) - (fee_asset + fee_asset2));

    }

    #[test]
    fn liq_price() {
        let config = Config::xbt_usd();
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());

        exchange.check_orders();

        assert_eq!(exchange.position.liq_price, Decimal::new(0, 0));

        // TODO: test liq_price with higher leverage and with short position as well
    }

    #[test]
    fn unrealized_pnl() {
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());

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
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 100.0);
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());
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
        let config = Config::xbt_usd();
        let mut exchange = Exchange::new(config);
        let t = Trade{
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 100.0);
        let order_err = exchange.submit_order(&o);
        assert!(order_err.is_none());

        // TODO: test cancel order
    }
}
