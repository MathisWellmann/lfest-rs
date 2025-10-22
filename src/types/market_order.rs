use getset::{
    CopyGetters,
    Getters,
};

use super::{
    Currency,
    ExchangeOrderMeta,
    Filled,
    Mon,
    OrderError,
    Pending,
    QuoteCurrency,
    Side,
    TimestampNs,
    UserOrderId,
    order_status::NewOrder,
};

/// Defines an market order aka taker order.
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
/// - `OrderStatus`: The status of the order for each stage, contains different information based on the stage.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct MarketOrder<I, const D: u8, BaseOrQuote, UserOrderIdT, OrderStatus>
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

    /// The amount of currency `S` the order is for and fill information.
    #[getset(get_copy = "pub")]
    quantity: BaseOrQuote,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,

    _quote: std::marker::PhantomData<QuoteCurrency<I, D>>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT, State> std::fmt::Display
    for MarketOrder<I, D, BaseOrQuote, UserOrderIdT, State>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
    State: Clone + std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "user_order_id: {:?}, side: {}, quantity: {}, state: {}",
            self.user_order_id, self.side, self.quantity, self.state
        )
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    MarketOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new market order without a `user_order_id`.
    ///
    /// # Arguments.
    /// - `side`: either buy or sell
    /// - `quantity`: A positive nonzero quantity of the amount of contracts this order is for.
    ///
    /// # Returns:
    /// Either a successfully created instance or an [`OrderError`]
    pub fn new(side: Side, quantity: BaseOrQuote) -> Result<Self, OrderError> {
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderError::OrderQuantityLTEZero);
        }
        Ok(MarketOrder {
            user_order_id: UserOrderIdT::default(),
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
        user_order_id: UserOrderIdT,
    ) -> Result<Self, OrderError> {
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
    ) -> MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>> {
        MarketOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            quantity: self.quantity,
            state: Pending::new(meta),
            _quote: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Mark the order as filled, by modifying its state.
    pub(crate) fn into_filled(
        self,
        fill_price: QuoteCurrency<I, D>,
        ts_ns_executed: TimestampNs,
    ) -> MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>> {
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        types::BaseCurrency,
        utils::NoUserOrderId,
    };

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn market_order_new(side: Side) {
        let _order =
            MarketOrder::<_, 5, _, NoUserOrderId, _>::new(side, BaseCurrency::<i64, 5>::new(5, 0))
                .unwrap();
        assert_eq!(
            MarketOrder::<_, 5, _, NoUserOrderId, _>::new(side, BaseCurrency::<i64, 5>::new(0, 0)),
            Err(OrderError::OrderQuantityLTEZero)
        );
        assert_eq!(
            MarketOrder::<_, 5, _, NoUserOrderId, _>::new(side, BaseCurrency::<i64, 5>::new(-5, 0)),
            Err(OrderError::OrderQuantityLTEZero)
        );
    }

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn market_order_new_with_user_order_id(side: Side) {
        let _order = MarketOrder::<_, 5, _, u64, _>::new_with_user_order_id(
            side,
            BaseCurrency::<i64, 5>::new(5, 0),
            1,
        )
        .unwrap();
        assert_eq!(
            MarketOrder::new_with_user_order_id(side, BaseCurrency::<i64, 5>::new(0, 0), 1),
            Err(OrderError::OrderQuantityLTEZero)
        );
        assert_eq!(
            MarketOrder::new_with_user_order_id(side, BaseCurrency::<i64, 5>::new(-5, 0), 1),
            Err(OrderError::OrderQuantityLTEZero)
        );
    }
}
