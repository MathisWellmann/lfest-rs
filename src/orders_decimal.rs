use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

#[derive(Debug)]
pub enum OrderError {
    MaxActiveOrders,
    InvalidOrder,
    InvalidPrice,
    InvalidTriggerPrice,
    InvalidOrderSize,
    NotEnoughAvailableBalance,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: u64,  // id will be filled in using exchange.submit_order()
    pub timestamp: u64,  // timestamp will be filled in using exchange.submit_order()
    pub order_type: OrderType,
    pub price: Decimal,
    pub price_opt: Decimal,
    pub size: Decimal,
    pub side: Side,
    done: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit,
    StopMarket,
    TakeProfitLimit,
    TakeProfitMarket,
}

impl Order {
    pub fn limit(side: Side, price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::Limit,
            price: Decimal::from_f64(price).unwrap(),
            price_opt: Decimal::new(0, 0),
            size: Decimal::from_f64(size).unwrap(),
            side,
            done: false,
        }
    }

    pub fn market(side: Side, size: f64) -> Order {
        return Order{
            id: 0,
            timestamp: 0,
            order_type: OrderType::Market,
            price: Decimal::new(0, 0),
            price_opt: Decimal::new(0, 0),
            size: Decimal::from_f64(size).unwrap(),
            side,
            done: false,
        }
    }

    pub fn stop_market(side: Side, trigger_price: f64, size: f64) -> Order {
        return Order{
            id: 0,
            timestamp: 0,
            order_type: OrderType::StopMarket,
            price: Decimal::from_f64(trigger_price).unwrap(),
            price_opt: Decimal::new(0, 0),
            size: Decimal::from_f64(size).unwrap(),
            side,
            done: false,
        }
    }

    pub fn take_profit_market(side: Side, trigger_price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::TakeProfitMarket,
            price: Decimal::from_f64(trigger_price).unwrap(),
            price_opt: Decimal::new(0, 0),
            size: Decimal::from_f64(size).unwrap(),
            side,
            done: false,
        }
    }

    pub fn take_profit_limit(side: Side, trigger_price: f64, limit_price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::TakeProfitLimit,
            price: Decimal::from_f64(trigger_price).unwrap(),
            price_opt: Decimal::from_f64(limit_price).unwrap(),
            size: Decimal::from_f64(size).unwrap(),
            side,
            done: false,
        }
    }

    pub fn mark_done(&mut self) {
        self.done = true;
    }

    pub fn done(&self) -> bool {
        return self.done
    }
}
