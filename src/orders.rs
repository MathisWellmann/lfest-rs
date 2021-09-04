use crate::{OrderError, OrderType, Side};

#[derive(Debug, Clone, Copy)]
/// Defines an order
pub struct Order {
    /// id will be filled in using exchange.submit_order()
    id: u64,
    /// timestamp will be filled in using exchange.submit_order()
    timestamp: i64,
    /// order type
    order_type: OrderType,
    /// the limit order price
    limit_price: Option<f64>,
    /// order size
    size: f64,
    /// order side
    side: Side,
    /// whether or not the order has been marked as executed
    executed: bool,
}

impl Order {
    /// Create a new limit order
    /// # Arguments
    /// - side: either buy or sell
    /// - limit_price: price to execute at or better
    /// - size: denoted in QUOTE currency when using linear futures
    //          denoted in BASE currency when using inverse futures
    /// # Returns
    /// Either a successfully created order or an OrderError
    #[must_use]
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
            limit_price: Some(limit_price),
            size,
            side,
            executed: false,
        })
    }

    /// Create a new market order
    /// # Arguments
    /// - side: either buy or sell
    /// - size: denoted in QUOTE currency when using linear futures
    ///         denoted in BASE currency when using inverse futures
    /// # Returns
    /// Either a successfully created order or an OrderError
    #[must_use]
    pub fn market(side: Side, size: f64) -> Result<Order, OrderError> {
        if size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        Ok(Order {
            id: 0,
            timestamp: 0,
            order_type: OrderType::Market,
            limit_price: None,
            size,
            side,
            executed: false,
        })
    }

    /// Id of Order
    #[inline(always)]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Timestamp of Order
    #[inline(always)]
    pub fn timestamp(&self) -> i64 {
        self.timestamp
    }

    /// OrderType of Order
    #[inline(always)]
    pub fn order_type(&self) -> OrderType {
        self.order_type
    }

    /// limit price of Order
    #[inline(always)]
    pub fn limit_price(&self) -> Option<f64> {
        self.limit_price
    }

    /// Size of Order denoted in QUOTE currency
    #[inline(always)]
    pub fn size(&self) -> f64 {
        self.size
    }

    /// Side of Order
    #[inline(always)]
    pub fn side(&self) -> Side {
        self.side
    }

    /// Execution status of Order
    #[inline(always)]
    pub fn executed(&self) -> bool {
        self.executed
    }

    /// Marks the order as executed
    #[inline(always)]
    pub(crate) fn mark_executed(&mut self) {
        self.executed = true;
    }

    #[inline(always)]
    pub(crate) fn set_id(&mut self, id: u64) {
        self.id = id
    }

    #[inline(always)]
    pub(crate) fn set_timestamp(&mut self, ts: i64) {
        self.timestamp = ts
    }
}
