pub enum OrderError {
    MaxActiveOrders,
    InvalidOrder,
    NotEnoughAvailableBalance,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: u64,  // id will be filled in using exchange.submit_order()
    pub timestamp: u64,  // timestamp will be filled in using exchange.submit_order()
    pub order_type: OrderType,
    pub price: f64,
    pub price_opt: f64,
    pub size: f64,
    pub side: Side,
    done: bool,
}

#[derive(Debug, Clone)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit,
    StopLimit,
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
            price,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn market(side: Side, price: f64, size: f64) -> Order {
        return Order{
            id: 0,
            timestamp: 0,
            order_type: OrderType::Market,
            price,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn stop_market(side: Side, price: f64, size: f64) -> Order {
        return Order{
            id: 0,
            timestamp: 0,
            order_type: OrderType::StopMarket,
            price,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn stop_limit(side: Side, trigger_price: f64, limit_price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::StopLimit,
            price: trigger_price,
            price_opt: limit_price,
            size,
            side,
            done: false,
        }
    }

    pub fn take_profit_market(side: Side, price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::TakeProfitMarket,
            price,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn take_profit_limit(side: Side, trigger_price: f64, limit_price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::TakeProfitLimit,
            price: trigger_price,
            price_opt: limit_price,
            size,
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
