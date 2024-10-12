use getset::{CopyGetters, Getters};
use num_traits::Zero;

use super::{
    order_status::NewOrder, CurrencyMarker, ExchangeOrderMeta, Filled, Mon, Monies, OrderError,
    Pending, Quote, Side, TimestampNs,
};

/// Defines an market order aka taker order.
/// Is generic over:
/// `S`: The order size aka quantity which is denoted in either base or quote currency.
/// `UserOrderId`: The type of user order id to use.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct MarketOrder<T, BaseOrQuote, UserOrderId, OrderStatus>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
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
    quantity: Monies<T, BaseOrQuote>,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,
}

impl<T, BaseOrQuote, UserOrderId> MarketOrder<T, BaseOrQuote, UserOrderId, NewOrder>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
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
    pub fn new(side: Side, quantity: Monies<T, BaseOrQuote>) -> Result<Self, OrderError<T>> {
        if quantity <= Monies::zero() {
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
        quantity: Monies<T, BaseOrQuote>,
        user_order_id: UserOrderId,
    ) -> Result<Self, OrderError<T>> {
        if quantity <= Monies::zero() {
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
    pub fn into_pending(
        self,
        meta: ExchangeOrderMeta,
    ) -> MarketOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>> {
        MarketOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            quantity: self.quantity,
            state: Pending::new(meta),
        }
    }
}

impl<T, BaseOrQuote, UserOrderId> MarketOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    UserOrderId: Clone,
{
    /// Mark the order as filled, by modifying its state.
    pub(crate) fn into_filled(
        self,
        fill_price: Monies<T, Quote>,
        ts_ns_executed: TimestampNs,
    ) -> MarketOrder<T, BaseOrQuote, UserOrderId, Filled<T, BaseOrQuote>> {
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
