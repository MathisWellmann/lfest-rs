use crate::{OrderError, OrderType, Side};

#[derive(Debug, Clone, Copy)]
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
    /// order size
    pub size: f64,
    /// order side
    pub side: Side,
    /// whether or not the order has been marked as executed
    pub executed: bool,
}

impl Order {
    /// Create a new limit order
    /// Returns an OrderError if either the limit_price or size is invalid
    pub fn limit(side: Side, limit_price: f64, size: f64) -> Result<Order, OrderError> {
        if limit_price <= 0.0 {
            return Err(OrderError::InvalidLimitPrice);
        }
        if size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        Ok(Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::Limit,
            limit_price,
            trigger_price: 0.0,
            size,
            side,
            executed: false,
        })
    }

    /// Create a new market order
    /// Returns an OrderError if wrong size provided
    pub fn market(side: Side, size: f64) -> Result<Order, OrderError> {
        if size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        Ok(Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::Market,
            limit_price: 0.0,
            trigger_price: 0.0,
            size,
            side,
            executed: false,
        })
    }

    /// Create a new stop market order
    /// Returns an OrderError if either the trigger_price or size is invalid
    pub fn stop_market(side: Side, trigger_price: f64, size: f64) -> Result<Order, OrderError> {
        if trigger_price <= 0.0 {
            return Err(OrderError::InvalidTriggerPrice);
        }
        if size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        Ok(Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::StopMarket,
            limit_price: 0.0,
            trigger_price,
            size,
            side,
            executed: false,
        })
    }

    /// Marks the order as executed
    #[inline(always)]
    pub(crate) fn mark_executed(&mut self) {
        self.executed = true;
    }
}
