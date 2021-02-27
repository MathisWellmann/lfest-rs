use crate::{OrderType, Side};

#[derive(Debug, Clone)]
/// Defines an order
pub struct Order {
    /// id will be filled in using exchange.submit_order()
    pub id: u64,
    /// timestamp will be filled in using exchange.submit_order()
    pub timestamp: u64,
    /// order type
    pub order_type: OrderType,
    /// the limit order price
    pub limit_price: f64,
    /// the trigger price
    pub trigger_price: f64,
    /// order size denoted in QUOTE currency
    pub size: f64,
    /// order side
    pub side: Side,
    /// whether or not the order has been marked as executed
    pub executed: bool,
}

impl Order {
    /// Create a new limit order
    pub fn limit(side: Side, limit_price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::Limit,
            limit_price,
            trigger_price: 0.0,
            size,
            side,
            executed: false,
        };
    }

    /// Create a new market order
    pub fn market(side: Side, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::Market,
            limit_price: 0.0,
            trigger_price: 0.0,
            size,
            side,
            executed: false,
        };
    }

    /// Create a new stop market order
    pub fn stop_market(side: Side, trigger_price: f64, size: f64) -> Order {
        return Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::StopMarket,
            limit_price: 0.0,
            trigger_price,
            size,
            side,
            executed: false,
        };
    }

    /// Marks the order as executed
    pub(crate) fn mark_executed(&mut self) {
        self.executed = true;
    }
}
