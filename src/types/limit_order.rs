use getset::{CopyGetters, Getters};

use super::{
    order_meta::ExchangeOrderMeta, order_status::NewOrder, Filled, MarginCurrency, OrderQuantity,
    Pending, TimestampNs,
};
use crate::types::{Currency, OrderError, QuoteCurrency, Side};

/// Defines a limit order.
/// Is generic over:
/// `S`: The order size aka quantity which is denoted in either base or quote currency.
/// `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct LimitOrder<Q, UserOrderId, OrderStatus>
where
    Q: Currency,
    UserOrderId: Clone,
    OrderStatus: Clone,
{
    /// Order Id provided by the user, can be any type really.
    #[getset(get = "pub")]
    user_order_id: UserOrderId,

    /// Whether its a buy or sell order.
    #[getset(get_copy = "pub")]
    side: Side,

    /// The limit order price, where it will sit in the orderbook.
    #[getset(get_copy = "pub")]
    limit_price: QuoteCurrency,

    /// The amount of Currency `S` the order is for and fill information.
    #[getset(get = "pub")]
    quantity: OrderQuantity<Q>,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,
}

impl<Q> LimitOrder<Q, (), NewOrder>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    /// Create a new limit order without a user_order_id.
    ///
    /// # Arguments:
    /// - `side`: either buy or sell
    /// - `limit_price`: price to execute at or better
    /// - `quantity`: A positive nonzero quantity of the amount of contracts this order is for.
    ///
    /// # Returns:
    /// Either a successfully created order or an [`OrderError`]
    pub fn new(side: Side, limit_price: QuoteCurrency, quantity: Q) -> Result<Self, OrderError> {
        if limit_price <= QuoteCurrency::new_zero() {
            return Err(OrderError::LimitPriceLTEZero);
        }
        if quantity <= Q::new_zero() {
            return Err(OrderError::OrderSizeLTEZero);
        }
        Ok(Self {
            user_order_id: (),
            state: NewOrder,
            limit_price,
            quantity: OrderQuantity::new_unfilled(quantity),
            side,
        })
    }
}

impl<Q, UserOrderId> LimitOrder<Q, UserOrderId, NewOrder>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone,
{
    /// Create a new limit order
    ///
    /// # Arguments:
    /// - `side`: either buy or sell
    /// - `limit_price`: price to execute at or better
    /// - `quantity`: How many contracts should be traded
    /// - `user_order_id`: The user provided id. This value is ignored by the exchange.
    ///
    /// # Returns:
    /// Either a successfully created order or an [`OrderError`]
    pub fn new_with_user_order_id(
        side: Side,
        limit_price: QuoteCurrency,
        quantity: Q,
        user_order_id: UserOrderId,
    ) -> Result<Self, OrderError> {
        if limit_price <= QuoteCurrency::new_zero() {
            return Err(OrderError::LimitPriceLTEZero);
        }
        if quantity <= Q::new_zero() {
            return Err(OrderError::OrderSizeLTEZero);
        }
        Ok(Self {
            user_order_id,
            state: NewOrder,
            limit_price,
            quantity: OrderQuantity::new_unfilled(quantity),
            side,
        })
    }

    /// Take in the order metadata provided by the exchange and coverts the order to the `Pending` state.
    pub(crate) fn into_pending(
        self,
        meta: ExchangeOrderMeta,
    ) -> LimitOrder<Q, UserOrderId, Pending> {
        LimitOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            limit_price: self.limit_price,
            quantity: self.quantity,
            state: Pending::new(meta),
        }
    }
}

impl<Q, UserOrderId> LimitOrder<Q, UserOrderId, Pending>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone,
{
    /// Mark the order as filled, by modifying its state.
    pub(crate) fn into_filled(
        self,
        fill_price: QuoteCurrency,
        ts_ns_executed: TimestampNs,
    ) -> LimitOrder<Q, UserOrderId, Filled> {
        let mut quantity = self.quantity;
        quantity.fill(fill_price);

        LimitOrder {
            user_order_id: self.user_order_id,
            state: Filled::new(self.state.meta().clone(), ts_ns_executed),
            limit_price: self.limit_price,
            quantity,
            side: self.side,
        }
    }
}
