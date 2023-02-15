use serde::{Deserialize, Serialize};

use crate::{OrderError, OrderType, QuoteCurrency, Side};

/// Defines an order
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Order<S> {
    /// id will be filled in using exchange.submit_order()
    id: u64,
    /// Order Id provided by user
    user_order_id: Option<u64>,
    /// timestamp will be filled in using exchange.submit_order()
    timestamp: i64,
    /// order type
    order_type: OrderType,
    /// the limit order price
    limit_price: Option<QuoteCurrency>,
    /// order size
    /// TODO: make type level machinery for this
    /// denoted in BASE currency if using linear futures,
    /// denoted in QUOTE currency if using inverse futures
    size: S,
    /// order side
    side: Side,
    /// whether or not the order has been marked as executed
    pub(crate) executed: bool,
}

impl<S> Order<S> {
    /// Create a new limit order
    ///
    /// # Arguments:
    /// - `side`: either buy or sell
    /// - `limit_price`: price to execute at or better
    /// - `size`: How many contracts should be traded
    ///
    /// # Returns:
    /// Either a successfully created order or an [`OrderError`]
    #[inline]
    pub fn limit(side: Side, limit_price: QuoteCurrency, size: S) -> Result<Self, OrderError> {
        if limit_price <= 0.0 {
            return Err(OrderError::InvalidLimitPrice);
        }
        if size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        Ok(Order {
            id: 0,
            user_order_id: None,
            timestamp: 0,
            order_type: OrderType::Limit,
            limit_price: Some(limit_price),
            size,
            side,
            executed: false,
        })
    }

    /// Create a new market order.
    ///
    /// # Arguments.
    /// - `side`: either buy or sell
    /// - `size`: How many contracts to trade
    ///
    /// # Returns:
    /// Either a successfully created instance or an [`OrderError`]
    #[inline]
    pub fn market(side: Side, size: S) -> Result<Self, OrderError> {
        if size <= 0.0 {
            return Err(OrderError::InvalidOrderSize);
        }
        Ok(Order {
            id: 0,
            user_order_id: None,
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

    /// User id of Order
    #[inline(always)]
    pub fn user_order_id(&self) -> &Option<u64> {
        &self.user_order_id
    }

    /// Set the user id of Order
    #[inline(always)]
    pub fn set_user_order_id(&mut self, id: u64) {
        self.user_order_id = Some(id)
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
    pub fn limit_price(&self) -> Option<QuoteCurrency> {
        self.limit_price
    }

    /// Size of Order
    #[inline(always)]
    pub fn size(&self) -> S {
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

    /// Set the timestamp of the order,
    /// note that the timestamps will be overwritten if set_order_timestamps is
    /// set in Config
    #[inline(always)]
    pub fn set_timestamp(&mut self, ts: i64) {
        self.timestamp = ts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, quote, BaseCurrency, QuoteCurrency};

    #[test]
    fn order_eq() {
        assert_eq!(
            Order::limit(Side::Buy, quote!(100.0), base!(100.0)).unwrap(),
            Order::limit(Side::Buy, quote!(100.0), base!(100.0)).unwrap()
        );
    }
}
