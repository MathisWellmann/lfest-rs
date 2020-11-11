use crate::{Side, OrderType};

#[derive(Debug, Clone)]
pub struct OrderFloat {
    pub id: u64,  // id will be filled in using exchange.submit_order()
    pub timestamp: u64,  // timestamp will be filled in using exchange.submit_order()
    pub order_type: OrderType,
    pub price: f64,
    pub price_opt: f64,
    pub size: f64,
    pub side: Side,
    done: bool,
}

impl OrderFloat {
    pub fn limit(side: Side, price: f64, size: f64) -> OrderFloat {
        return OrderFloat {
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

    pub fn market(side: Side, size: f64) -> OrderFloat {
        return OrderFloat{
            id: 0,
            timestamp: 0,
            order_type: OrderType::Market,
            price: 0.0,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn stop_market(side: Side, trigger_price: f64, size: f64) -> OrderFloat {
        return OrderFloat{
            id: 0,
            timestamp: 0,
            order_type: OrderType::StopMarket,
            price: trigger_price,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn take_profit_market(side: Side, trigger_price: f64, size: f64) -> OrderFloat {
        return OrderFloat {
            id: 0,
            timestamp: 0,
            order_type: OrderType::TakeProfitMarket,
            price: trigger_price,
            price_opt: 0.0,
            size,
            side,
            done: false,
        }
    }

    pub fn take_profit_limit(side: Side, trigger_price: f64, limit_price: f64, size: f64) -> OrderFloat {
        return OrderFloat {
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
