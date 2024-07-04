use getset::{CopyGetters, Getters};

use super::{
    order_status::NewOrder, Currency, ExchangeOrderMeta, Filled, MarginCurrency, OrderError,
    Pending, QuoteCurrency, Side, TimestampNs,
};

/// Defines an market order aka taker order.
/// Is generic over:
/// `S`: The order size aka quantity which is denoted in either base or quote currency.
/// `UserOrderId`: The type of user order id to use.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct MarketOrder<Q, UserOrderId, OrderStatus>
where
    Q: Currency,
    OrderStatus: Clone,
{
    /// Order Id provided by the user, can be any type really.
    #[getset(get = "pub")]
    user_order_id: UserOrderId,

    /// Whether its a buy or sell order.
    #[getset(get_copy = "pub")]
    side: Side,

    /// The amount of currency `S` the order is for and fill information.
    #[getset(get_copy = "pub")]
    quantity: Q,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,
}

impl<Q, UserOrderId> MarketOrder<Q, UserOrderId, NewOrder>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Default,
{
    /// Create a new market order without a `user_order_id`.
    ///
    /// # Arguments.
    /// - `side`: either buy or sell
    /// - `quantity`: A positive nonzero quantity of the amount of contracts this order is for.
    ///
    /// # Returns:
    /// Either a successfully created instance or an [`OrderError`]
    pub fn new(side: Side, quantity: Q) -> Result<Self, OrderError> {
        if quantity <= Q::new_zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(MarketOrder {
            user_order_id: UserOrderId::default(),
            state: NewOrder,
            side,
            quantity,
        })
    }

    /// Create a new limit order
    ///
    /// # Arguments:
    /// - `side`: either buy or sell
    /// - `size`: How many contracts should be traded
    /// - `user_order_id`: The user provided id. This value is ignored by the exchange.
    ///
    /// # Returns:
    /// Either a successfully created order or an [`OrderError`]
    pub fn new_with_user_order_id(
        side: Side,
        quantity: Q,
        user_order_id: UserOrderId,
    ) -> Result<Self, OrderError> {
        if quantity <= Q::new_zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(Self {
            user_order_id,
            state: NewOrder,
            quantity,
            side,
        })
    }

    /// Take in the order metadata provided by the exchange and coverts the order to the `Pending` state.
    pub fn into_pending(self, meta: ExchangeOrderMeta) -> MarketOrder<Q, UserOrderId, Pending<Q>> {
        MarketOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            quantity: self.quantity,
            state: Pending::new(meta),
        }
    }
}

impl<Q, UserOrderId> MarketOrder<Q, UserOrderId, Pending<Q>>
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
    ) -> MarketOrder<Q, UserOrderId, Filled<Q>> {
        MarketOrder {
            user_order_id: self.user_order_id,
            state: Filled::new(
                self.state.meta().clone(),
                ts_ns_executed,
                fill_price,
                // Market orders are always fully filled currently.
                self.quantity,
            ),
            quantity: self.quantity,
            side: self.side,
        }
    }
}
