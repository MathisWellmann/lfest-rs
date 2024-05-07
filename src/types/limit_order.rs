use getset::{CopyGetters, Getters};

use super::{
    order_meta::ExchangeOrderMeta, order_status::NewOrder, Filled, FilledQuantity, MarginCurrency,
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

    /// The remaining amount of Currency `S` the order is for.
    #[getset(get_copy = "pub")]
    remaining_quantity: Q,

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
            remaining_quantity: quantity,
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
            remaining_quantity: quantity,
            side,
        })
    }

    /// Take in the order metadata provided by the exchange and coverts the order to the `Pending` state.
    pub(crate) fn into_pending(
        self,
        meta: ExchangeOrderMeta,
    ) -> LimitOrder<Q, UserOrderId, Pending<Q>> {
        LimitOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            limit_price: self.limit_price,
            remaining_quantity: self.remaining_quantity,
            state: Pending::new(meta),
        }
    }
}

impl<Q, UserOrderId> LimitOrder<Q, UserOrderId, Pending<Q>>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone,
{
    /// Used when an order gets some `quantity` filled at a `price`.
    ///
    /// # Returns:
    /// Some(filled_order), if the order is fully filled.
    pub(crate) fn fill(
        &mut self,
        filled_quantity: Q,
        ts_ns: TimestampNs,
    ) -> Option<LimitOrder<Q, UserOrderId, Filled>> {
        debug_assert!(
            filled_quantity <= self.remaining_quantity,
            "The filled quantity can not be greater than the limit order quantity"
        );
        debug_assert!(
            filled_quantity > Q::new_zero(),
            "Filled `quantity` must be greater than zero."
        );
        let price = self.limit_price();

        let fully_filled = match &mut self.state.filled_quantity {
            FilledQuantity::Unfilled => {
                self.state.filled_quantity = FilledQuantity::Filled {
                    cumulative_qty: filled_quantity,
                    avg_price: price,
                };
                filled_quantity == self.remaining_quantity
            }
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price,
            } => {
                debug_assert!(filled_quantity <= (self.remaining_quantity - *cumulative_qty), "The filled quantity can not be greater than the outstanding limit order quantity.");

                let new_qty = *cumulative_qty + filled_quantity;
                *avg_price = QuoteCurrency::new(
                    ((*cumulative_qty.as_ref() * *avg_price.as_ref())
                        + (*price.as_ref() * *filled_quantity.as_ref()))
                        / *new_qty.as_ref(),
                );
                *cumulative_qty = new_qty;

                *cumulative_qty == self.remaining_quantity
            }
        };

        if fully_filled {
            Some(LimitOrder {
                user_order_id: self.user_order_id.clone(),
                state: Filled::new(self.state.meta().clone(), ts_ns, price),
                limit_price: self.limit_price,
                remaining_quantity: self.remaining_quantity,
                side: self.side,
            })
        } else {
            None
        }
    }

    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> Q {
        match self.state.filled_quantity {
            FilledQuantity::Unfilled => self.remaining_quantity,
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price: _,
            } => self.remaining_quantity + cumulative_qty,
        }
    }

    /// Get the order id assigned by the exchange.
    pub fn id(&self) -> u64 {
        self.state().meta().id()
    }
}
