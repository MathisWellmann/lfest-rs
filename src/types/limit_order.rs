use getset::{CopyGetters, Getters, Setters};
use num_traits::Zero;

use super::{
    Currency, Filled, FilledQuantity, LimitOrderFill, MarginCurrency, Mon, OrderId, Pending,
    QuoteCurrency, RePricing, TimestampNs, UserOrderId, order_meta::ExchangeOrderMeta,
    order_status::NewOrder,
};
use crate::{
    types::{OrderError, Side},
    utils::NoUserOrderId,
};

/// Defines a limit order.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
/// - `OrderStatus`: The status of the order for each stage, contains different information based on the stage.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters, Setters)]
pub struct LimitOrder<I, const D: u8, BaseOrQuote, UserOrderIdT, OrderStatus>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
    OrderStatus: Clone,
{
    /// Order Id provided by the user, can be any type really.
    #[getset(get_copy = "pub")]
    user_order_id: UserOrderIdT,

    /// Whether its a buy or sell order.
    #[getset(get_copy = "pub")]
    side: Side,

    /// The limit order price, where it will sit in the orderbook.
    #[getset(get_copy = "pub")]
    limit_price: QuoteCurrency<I, D>,

    /// The remaining amount of Currency `S` the order is for.
    #[getset(get_copy = "pub")]
    remaining_quantity: BaseOrQuote,

    /// Determines the behaviour for when the limit price locks or crosses an away market quotation.
    #[getset(get_copy = "pub", set = "pub")]
    re_pricing: RePricing,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT, OrderStatus> std::fmt::Display
    for LimitOrder<I, D, BaseOrQuote, UserOrderIdT, OrderStatus>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
    OrderStatus: Clone + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "user_id: {:?}, limit {} {} @ {}, state: {:?}",
            self.user_order_id, self.side, self.remaining_quantity, self.limit_price, self.state
        )
    }
}

impl<I, const D: u8, BaseOrQuote> LimitOrder<I, D, BaseOrQuote, NoUserOrderId, NewOrder>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
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
    pub fn new(
        side: Side,
        limit_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
    ) -> Result<Self, OrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(OrderError::LimitPriceLTEZero);
        }
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(Self {
            user_order_id: NoUserOrderId,
            state: NewOrder,
            limit_price,
            remaining_quantity: quantity,
            side,
            re_pricing: RePricing::default(),
        })
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
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
        limit_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, OrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(OrderError::LimitPriceLTEZero);
        }
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(Self {
            user_order_id,
            state: NewOrder,
            limit_price,
            remaining_quantity: quantity,
            side,
            re_pricing: RePricing::default(),
        })
    }

    /// Set the `UserOrderId`.
    #[inline]
    pub fn set_user_order_id(&mut self, user_order_id: UserOrderIdT) {
        self.user_order_id = user_order_id;
    }

    /// Get the total quantity that this order is for.
    #[inline]
    pub fn total_quantity(&self) -> BaseOrQuote {
        self.remaining_quantity
    }

    /// Take in the order metadata provided by the exchange and coverts the order to the `Pending` state.
    pub fn into_pending(
        self,
        meta: ExchangeOrderMeta,
    ) -> LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>> {
        LimitOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            limit_price: self.limit_price,
            remaining_quantity: self.remaining_quantity,
            state: Pending::new(meta),
            re_pricing: RePricing::default(),
        }
    }

    /// Modify the `remaining_quantity`.
    /// The `new_qty` must be GT than zero.
    pub(crate) fn set_remaining_quantity(&mut self, new_qty: BaseOrQuote) {
        assert!(new_qty > BaseOrQuote::zero());
        self.remaining_quantity = new_qty;
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Used when an order gets some `quantity` filled at a `price`.
    pub(crate) fn fill(
        &mut self,
        filled_quantity: BaseOrQuote,
        ts_ns: TimestampNs,
    ) -> LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT> {
        debug_assert!(
            filled_quantity <= self.remaining_quantity,
            "The filled quantity can not be greater than the limit order quantity"
        );
        debug_assert!(
            filled_quantity > BaseOrQuote::zero(),
            "Filled quantity must be greater than zero."
        );
        let fill_price = self.limit_price();
        let meta = self.state.meta().clone();

        match &mut self.state.filled_quantity {
            FilledQuantity::Unfilled => {
                self.state.filled_quantity = FilledQuantity::Filled {
                    cumulative_qty: filled_quantity,
                    avg_price: fill_price,
                };
                if filled_quantity == self.remaining_quantity {
                    // Order fills in one go.
                    self.remaining_quantity -= filled_quantity;
                    let order_after_fill = LimitOrder {
                        user_order_id: self.user_order_id,
                        state: Filled::new(meta, ts_ns, fill_price, filled_quantity),
                        limit_price: self.limit_price,
                        remaining_quantity: BaseOrQuote::zero(),
                        side: self.side,
                        re_pricing: self.re_pricing,
                    };
                    return LimitOrderFill::FullyFilled {
                        fill_price,
                        filled_quantity,
                        order_after_fill,
                    };
                } else {
                    self.remaining_quantity -= filled_quantity;
                }
            }
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price,
            } => {
                let new_qty = *cumulative_qty + filled_quantity;
                *avg_price = QuoteCurrency::new_weighted_price(
                    *avg_price,
                    *cumulative_qty.as_ref(),
                    fill_price,
                    *filled_quantity.as_ref(),
                );
                *cumulative_qty = new_qty;

                self.remaining_quantity -= filled_quantity;

                if self.remaining_quantity.is_zero() {
                    let order_after_fill = LimitOrder {
                        user_order_id: self.user_order_id,
                        state: Filled::new(meta, ts_ns, fill_price, *cumulative_qty),
                        limit_price: self.limit_price,
                        remaining_quantity: BaseOrQuote::zero(),
                        side: self.side,
                        re_pricing: self.re_pricing,
                    };
                    return LimitOrderFill::FullyFilled {
                        fill_price,
                        filled_quantity,
                        order_after_fill,
                    };
                }
            }
        };

        LimitOrderFill::PartiallyFilled {
            fill_price,
            filled_quantity,
            order_after_fill: self.clone(),
        }
    }

    /// Get the total filled quantity for this order.
    pub fn filled_quantity(&self) -> BaseOrQuote {
        match self.state.filled_quantity {
            FilledQuantity::Unfilled => BaseOrQuote::zero(),
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price: _,
            } => cumulative_qty,
        }
    }

    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> BaseOrQuote {
        let q = match self.state.filled_quantity {
            FilledQuantity::Unfilled => self.remaining_quantity,
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price: _,
            } => self.remaining_quantity + cumulative_qty,
        };
        assert!(
            q > BaseOrQuote::zero(),
            "total quantity must always be > zero"
        );
        q
    }

    /// Get the order id assigned by the exchange.
    #[inline]
    pub fn id(&self) -> OrderId {
        self.state().meta().id()
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> BaseOrQuote {
        let q = self.state.filled_qty();
        assert!(
            q > BaseOrQuote::zero(),
            "total quantity must always be > zero"
        );
        q
    }
}

#[cfg(test)]
mod tests {
    use test_case::test_matrix;

    use super::*;
    use crate::{prelude::QuoteCurrency, types::BaseCurrency};

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_full(side: Side, limit_price: u32, qty: i32) {
        let limit_price = QuoteCurrency::<i32, 4>::new(limit_price as i32, 0);
        let qty = QuoteCurrency::new(qty, 0);
        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());

        let mut order = order.into_pending(meta.clone());
        let LimitOrderFill::FullyFilled {
            fill_price,
            filled_quantity,
            order_after_fill,
        } = order.fill(qty, 0.into())
        else {
            panic!("expected other update");
        };
        assert_eq!(fill_price, limit_price);
        assert_eq!(filled_quantity, qty);
        assert_eq!(
            order_after_fill.state(),
            &Filled::new(meta, 0.into(), limit_price, qty)
        );
        assert_eq!(order_after_fill.total_quantity(), qty);
        assert_eq!(order_after_fill.remaining_quantity(), QuoteCurrency::zero());
    }

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_partial(side: Side, limit_price: u32, qty: i32) {
        let limit_price = QuoteCurrency::<i32, 4>::new(limit_price as _, 0);
        let quantity = QuoteCurrency::new(qty, 0);
        let order = LimitOrder::new(side, limit_price, quantity).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());

        let qty = quantity / QuoteCurrency::new(2, 0);
        let mut order = order.into_pending(meta.clone());
        assert!(matches!(
            order.fill(qty, 0.into()),
            LimitOrderFill::PartiallyFilled {
                fill_price: limit_price,
                filled_quantity: qty,
                ..
            }
        ));
        let mut expected_state = Pending::new(meta);
        expected_state.filled_quantity = FilledQuantity::Filled {
            cumulative_qty: qty,
            avg_price: limit_price,
        };
        assert_eq!(order.state(), &expected_state);
    }

    #[test]
    fn size_of_limit_order() {
        assert_eq!(
            std::mem::size_of::<LimitOrder<i64, 5, BaseCurrency<i64, 5>, i64, NewOrder>>(),
            32
        );
        assert_eq!(
            std::mem::size_of::<LimitOrder<i32, 2, BaseCurrency<i32, 2>, i64, NewOrder>>(),
            24
        );
        assert_eq!(
            std::mem::size_of::<LimitOrder<i32, 2, BaseCurrency<i32, 2>, i32, NewOrder>>(),
            16
        );
        assert_eq!(
            std::mem::size_of::<
                LimitOrder<
                    i64,
                    2,
                    BaseCurrency<i64, 2>,
                    i64,
                    Pending<i64, 2, BaseCurrency<i64, 2>>,
                >,
            >(),
            72
        );
    }

    #[test]
    fn limit_order_new_with_user_order_id() {
        let order = LimitOrder::new_with_user_order_id(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
            1,
        )
        .unwrap();
        assert_eq!(order.user_order_id(), 1);
    }
}
