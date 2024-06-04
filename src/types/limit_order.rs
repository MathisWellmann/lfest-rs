use getset::{CopyGetters, Getters};

use super::{
    order_meta::ExchangeOrderMeta, order_status::NewOrder, Filled, FilledQuantity, MarginCurrency,
    OrderId, Pending, TimestampNs,
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
    UserOrderId: Clone + Default,
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

    /// Set the `UserOrderId`.
    pub fn set_user_order_id(&mut self, user_order_id: UserOrderId) {
        self.user_order_id = user_order_id;
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
    ) -> Option<LimitOrder<Q, UserOrderId, Filled<Q>>> {
        assert!(
            filled_quantity <= self.remaining_quantity,
            "The filled quantity can not be greater than the limit order quantity"
        );
        assert!(
            filled_quantity > Q::new_zero(),
            "Filled quantity must be greater than zero."
        );
        let price = self.limit_price();
        let meta = self.state.meta().clone();

        match &mut self.state.filled_quantity {
            FilledQuantity::Unfilled => {
                self.state.filled_quantity = FilledQuantity::Filled {
                    cumulative_qty: filled_quantity,
                    avg_price: price,
                };
                if filled_quantity == self.remaining_quantity {
                    self.remaining_quantity -= filled_quantity;
                    return Some(LimitOrder {
                        user_order_id: self.user_order_id.clone(),
                        state: Filled::new(
                            self.state.meta().clone(),
                            ts_ns,
                            price,
                            filled_quantity,
                        ),
                        limit_price: self.limit_price,
                        remaining_quantity: Q::new_zero(),
                        side: self.side,
                    });
                }
            }
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price,
            } => {
                let new_qty = *cumulative_qty + filled_quantity;
                *avg_price = QuoteCurrency::new(
                    ((*cumulative_qty.as_ref() * *avg_price.as_ref())
                        + (*price.as_ref() * *filled_quantity.as_ref()))
                        / *new_qty.as_ref(),
                );
                *cumulative_qty = new_qty;

                if *cumulative_qty == self.remaining_quantity {
                    self.remaining_quantity -= filled_quantity;
                    return Some(LimitOrder {
                        user_order_id: self.user_order_id.clone(),
                        state: Filled::new(meta, ts_ns, price, *cumulative_qty),
                        limit_price: self.limit_price,
                        remaining_quantity: Q::new_zero(),
                        side: self.side,
                    });
                }
            }
        };
        self.remaining_quantity -= filled_quantity;

        None
    }

    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> Q {
        let q = match self.state.filled_quantity {
            FilledQuantity::Unfilled => self.remaining_quantity,
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price: _,
            } => self.remaining_quantity + cumulative_qty,
        };
        assert!(q > Q::new_zero(), "total quantity must always be > zero");
        q
    }

    /// Get the order id assigned by the exchange.
    pub fn id(&self) -> OrderId {
        self.state().meta().id()
    }
}

impl<Q, UserOrderId> LimitOrder<Q, UserOrderId, Filled<Q>>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone,
{
    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> Q {
        let q = self.state.filled_qty();
        assert!(q > Q::new_zero(), "total quantity must always be > zero");
        q
    }
}

#[cfg(test)]
mod tests {
    use fpdec::{Dec, Decimal};
    use test_case::test_matrix;

    use super::*;

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_full(side: Side, limit_price: u32, qty: u32) {
        let limit_price = QuoteCurrency::from(Decimal::from(limit_price));
        let qty = QuoteCurrency::from(Decimal::from(qty));
        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0);

        let mut order = order.into_pending(meta.clone());
        let filled_order = order.fill(qty, 0).unwrap();
        assert_eq!(
            filled_order.state(),
            &Filled::new(meta, 0, limit_price, qty)
        );
        assert_eq!(filled_order.total_quantity(), qty);
    }

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_partial(side: Side, limit_price: u32, qty: u32) {
        let limit_price = QuoteCurrency::from(Decimal::from(limit_price));
        let qty = QuoteCurrency::from(Decimal::from(qty));
        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0);

        let qty = QuoteCurrency::from(Decimal::from(qty) / Dec!(2));
        let mut order = order.into_pending(meta.clone());
        assert!(order.fill(qty, 0).is_none());
        let mut expected_state = Pending::new(meta);
        expected_state.filled_quantity = FilledQuantity::Filled {
            cumulative_qty: qty,
            avg_price: limit_price,
        };
        assert_eq!(order.state(), &expected_state);
    }
}
