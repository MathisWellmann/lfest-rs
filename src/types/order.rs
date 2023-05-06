use crate::types::{Currency, OrderError, OrderType, QuoteCurrency, Side};

/// Defines an order
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The amount of Currency `S` the order is for
    quantity: S,
    /// order side
    side: Side,
    /// whether or not the order has been marked as executed
    pub(crate) executed: bool,
}

impl<S> Order<S>
where
    S: Currency,
{
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
        if limit_price <= QuoteCurrency::new_zero() {
            return Err(OrderError::LimitPriceTooLow);
        }
        if size <= S::new_zero() {
            return Err(OrderError::OrderSizeMustBePositive);
        }
        Ok(Order {
            id: 0,
            user_order_id: None,
            timestamp: 0,
            order_type: OrderType::Limit,
            limit_price: Some(limit_price),
            quantity: size,
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
        if size <= S::new_zero() {
            return Err(OrderError::OrderSizeMustBePositive);
        }
        Ok(Order {
            id: 0,
            user_order_id: None,
            timestamp: 0,
            order_type: OrderType::Market,
            limit_price: None,
            quantity: size,
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

    /// Quantity of Order
    #[inline(always)]
    pub fn quantity(&self) -> S {
        self.quantity
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
    use crate::prelude::*;

    #[test]
    fn order_eq() {
        assert_eq!(
            Order::limit(Side::Buy, quote!(100.0), base!(100.0)).unwrap(),
            Order::limit(Side::Buy, quote!(100.0), base!(100.0)).unwrap()
        );
    }
}
