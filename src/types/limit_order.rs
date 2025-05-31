use std::cmp::Ordering;

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

/// Price time priority ordering
pub fn price_time_priority_ordering<I, const D: u8, BaseOrQuote, UserOrderIdT>(
    o0: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    o1: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
) -> Ordering
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    use Ordering::*;
    match o0.limit_price().cmp(&o1.limit_price()) {
        Less => Less,
        Equal => {
            match o0
                .state()
                .meta()
                .ts_ns_exchange_received()
                .cmp(&o1.state().meta().ts_ns_exchange_received())
            {
                Less => Less,
                Equal => Equal,
                Greater => Greater,
            }
        }
        Greater => Ordering::Greater,
    }
}

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
{
    /// Order Id provided by the user, can be any type really.
    #[getset(get_copy = "pub")]
    user_order_id: UserOrderIdT,

    /// Whether its a buy or sell order.
    #[getset(get_copy = "pub")]
    side: Side,

    /// The limit order price, where it will sit in the order book.
    #[getset(get_copy = "pub")]
    limit_price: QuoteCurrency<I, D>,

    /// The remaining amount of Currency `S` the order is for.
    #[getset(get_copy = "pub")]
    remaining_quantity: BaseOrQuote,

    /// Determines the behavior for when the limit price locks or crosses an away market quotation.
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
    /// Used when an order gets some `quantity` filled at its limit price.
    pub(crate) fn fill(
        &mut self,
        filled_quantity: BaseOrQuote,
        fee: BaseOrQuote::PairedCurrency,
        ts_ns: TimestampNs,
    ) -> LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT> {
        assert2::debug_assert!(
            filled_quantity <= self.remaining_quantity,
            "The filled quantity can not be greater than the limit order quantity"
        );
        assert2::debug_assert!(
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
                        fee,
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
                        fee,
                        order_after_fill,
                    };
                }
            }
        };

        LimitOrderFill::PartiallyFilled {
            fill_price,
            filled_quantity,
            fee,
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

    /// The notional value is related to the quantity and its limit price.
    #[inline]
    pub fn notional(&self) -> BaseOrQuote::PairedCurrency {
        BaseOrQuote::PairedCurrency::convert_from(self.remaining_quantity, self.limit_price)
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
    use crate::{DECIMALS, prelude::QuoteCurrency, test_fee_maker, types::BaseCurrency};

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_full(side: Side, limit_price: u32, qty: i64) {
        let limit_price = QuoteCurrency::<i64, DECIMALS>::new(limit_price as i64, 0);
        let qty = QuoteCurrency::new(qty, 0);
        let f = BaseCurrency::convert_from(qty, limit_price) * *test_fee_maker().as_ref();
        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());

        let mut order = order.into_pending(meta.clone());

        let LimitOrderFill::FullyFilled {
            fill_price,
            filled_quantity,
            fee,
            order_after_fill,
        } = order.fill(qty, f, 0.into())
        else {
            panic!("expected other update");
        };
        assert_eq!(fill_price, limit_price);
        assert_eq!(filled_quantity, qty);
        assert_eq!(fee, f);
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
    fn limit_order_fill_partial(side: Side, limit_price: i64, qty: i64) {
        let limit_price = QuoteCurrency::<i64, DECIMALS>::new(limit_price, 0);
        let quantity = QuoteCurrency::new(qty, 0);
        let f = BaseCurrency::convert_from(quantity, limit_price) * *test_fee_maker().as_ref();
        let order = LimitOrder::new(side, limit_price, quantity).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());

        let qty = quantity / QuoteCurrency::new(2, 0);
        let mut order = order.into_pending(meta.clone());
        let LimitOrderFill::PartiallyFilled {
            fill_price,
            filled_quantity,
            fee,
            order_after_fill: _,
        } = order.fill(qty, f, 0.into())
        else {
            panic!("Expected `PartiallyFilled`");
        };
        assert_eq!(fill_price, limit_price);
        assert_eq!(filled_quantity, qty);
        assert_eq!(fee, f);

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
