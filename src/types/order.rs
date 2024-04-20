use getset::{CopyGetters, Getters};

use crate::types::{Currency, OrderError, OrderType, QuoteCurrency, Side};

/// Defines an order
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct Order<S: Currency> {
    /// id will be filled in using exchange.submit_order()
    #[getset(get_copy = "pub")]
    id: u64,

    /// Order Id provided by user
    #[getset(get_copy = "pub")]
    user_order_id: Option<u64>,
    /// timestamp will be filled in using exchange.submit_order()

    #[getset(get_copy = "pub")]
    timestamp: i64,

    /// order type
    #[getset(get = "pub")]
    order_type: OrderType,

    /// the limit order price
    #[getset(get_copy = "pub")]
    limit_price: Option<QuoteCurrency>,

    /// The amount of Currency `S` the order is for
    #[getset(get_copy = "pub")]
    quantity: S,

    /// whether or not the order has been executed
    #[getset(get_copy = "pub")]
    pub(crate) filled: Filled,
}

/// Whether the order has been executed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filled {
    /// The order has not been filled yet
    No,
    /// The order has been filled
    Yes {
        /// The average price this order has been filled at
        fill_price: QuoteCurrency,
    },
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
            return Err(OrderError::LimitPriceBelowZero);
        }
        if size <= S::new_zero() {
            return Err(OrderError::OrderSizeMustBePositive);
        }
        Ok(Order {
            id: 0,
            user_order_id: None,
            timestamp: 0,
            order_type: OrderType::Limit { side, limit_price },
            limit_price: Some(limit_price),
            quantity: size,
            filled: Filled::No,
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
            order_type: OrderType::Market { side },
            limit_price: None,
            quantity: size,
            filled: Filled::No,
        })
    }

    /// Side of Order
    #[inline]
    pub fn side(&self) -> Side {
        match self.order_type {
            OrderType::Market { side } => side,
            OrderType::Limit {
                side,
                limit_price: _,
            } => side,
        }
    }

    /// Marks the order as filled at the `fill_price`
    #[inline]
    pub(crate) fn mark_filled(&mut self, fill_price: QuoteCurrency) {
        self.filled = Filled::Yes { fill_price }
    }

    #[inline]
    pub(crate) fn set_id(&mut self, id: u64) {
        self.id = id
    }

    /// Set the timestamp of the order,
    /// note that the timestamps will be overwritten if set_order_timestamps is
    /// set in Config
    #[inline]
    pub fn set_timestamp(&mut self, ts: i64) {
        self.timestamp = ts
    }
}
