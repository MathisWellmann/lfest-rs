use getset::{CopyGetters, Getters};

use super::{
    order_status::NewOrder, Currency, ExchangeOrderMeta, Filled, Mon, OrderError, Pending,
    QuoteCurrency, Side, TimestampNs,
};

/// Defines an market order aka taker order.
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
/// - `OrderStatus`: The status of the order for each stage, contains different information based on the stage.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct MarketOrder<I, const D: u8, BaseOrQuote, UserOrderId, OrderStatus>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
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
    quantity: BaseOrQuote,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,

    _quote: std::marker::PhantomData<QuoteCurrency<I, D>>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> MarketOrder<I, D, BaseOrQuote, UserOrderId, NewOrder>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
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
    pub fn new(side: Side, quantity: BaseOrQuote) -> Result<Self, OrderError<I, D>> {
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(MarketOrder {
            user_order_id: UserOrderId::default(),
            state: NewOrder,
            side,
            quantity,
            _quote: std::marker::PhantomData,
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
        quantity: BaseOrQuote,
        user_order_id: UserOrderId,
    ) -> Result<Self, OrderError<I, D>> {
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(Self {
            user_order_id,
            state: NewOrder,
            quantity,
            side,
            _quote: std::marker::PhantomData,
        })
    }

    /// Take in the order metadata provided by the exchange and coverts the order to the `Pending` state.
    pub fn into_pending(
        self,
        meta: ExchangeOrderMeta,
    ) -> MarketOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>> {
        MarketOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            quantity: self.quantity,
            state: Pending::new(meta),
            _quote: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderId>
    MarketOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderId: Clone,
{
    /// Mark the order as filled, by modifying its state.
    pub(crate) fn into_filled(
        self,
        fill_price: QuoteCurrency<I, D>,
        ts_ns_executed: TimestampNs,
    ) -> MarketOrder<I, D, BaseOrQuote, UserOrderId, Filled<I, D, BaseOrQuote>> {
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
            _quote: std::marker::PhantomData,
        }
    }
}
