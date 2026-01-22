use std::num::NonZeroU16;

use const_decimal::Decimal;
use getset::Getters;
use num::{
    One,
    Zero,
};
use tracing::debug;

use crate::{
    account::{
        Asks,
        Bids,
        SortedOrders,
    },
    prelude::Position,
    types::{
        CancelBy,
        Currency,
        Filled,
        LimitOrder,
        MarginCurrency,
        MaxNumberOfActiveOrders,
        Mon,
        OrderId,
        OrderIdNotFound,
        Pending,
        Side::{
            self,
            *,
        },
        TimestampNs,
        UserOrderId,
    },
    utils::order_margin,
};

/// The datatype that holds the active limit orders of a user.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currency.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderIdT`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Clone, Debug, PartialEq, Eq, Getters)]
pub struct ActiveLimitOrders<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Stores all the active buy orders in ascending price priority.
    /// Best bid with oldest timestamp is the last element.
    #[getset(get = "pub")]
    bids: SortedOrders<I, D, BaseOrQuote, UserOrderIdT, Bids>,

    /// Stores all the active sell orders in descending price priority.
    /// Best ask with oldest timestamp is the last element.
    #[getset(get = "pub")]
    asks: SortedOrders<I, D, BaseOrQuote, UserOrderIdT, Asks>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new order book instance with a maximum capacity for bids and asks.
    /// The `max_active_orders` must be non zero as we need at least space for one limit order.
    pub(crate) fn with_capacity(max_active_orders_per_side: NonZeroU16) -> Self {
        Self {
            bids: SortedOrders::with_capacity(max_active_orders_per_side),
            asks: SortedOrders::with_capacity(max_active_orders_per_side),
        }
    }

    /// Get the number of active limit orders.
    #[inline(always)]
    pub fn num_active(&self) -> usize {
        self.bids.len() + self.asks.len()
    }

    /// `true` is there are no active orders.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// The best bid has the highest limit price of all buy orders and the oldest timestamp.
    #[inline(always)]
    #[must_use]
    pub fn best_bid(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.bids.best()
    }

    /// The best ask has the lowest limit price of all sell orders and the oldest timestamp.
    #[inline(always)]
    #[must_use]
    pub fn best_ask(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.asks.best()
    }

    /// Try to insert a new `LimitOrder` into the order book.
    /// Returns an error if the maximum capacity is reached.
    #[inline(always)]
    pub(crate) fn try_insert(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        match order.side() {
            Buy => self.bids.try_insert(order),
            Sell => self.asks.try_insert(order),
        }
    }

    #[inline(always)]
    #[must_use]
    pub(crate) fn order_margin(
        &self,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        order_margin(
            self.bids.notional_sum(),
            self.asks.notional_sum(),
            init_margin_req,
            position,
        )
    }

    /// Get the order margin if a new order were to be added.
    #[must_use]
    pub(crate) fn order_margin_with_order(
        &self,
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(init_margin_req > Decimal::zero());
        assert2::debug_assert!(init_margin_req <= Decimal::one());

        let mut buy_notional = self.bids.notional_sum();
        let mut sell_notional = self.asks.notional_sum();
        let new_notional = new_order.notional();
        match new_order.side() {
            Buy => buy_notional += new_notional,
            Sell => sell_notional += new_notional,
        }

        order_margin(buy_notional, sell_notional, init_margin_req, position)
    }

    /// Get a `LimitOrder` by the given `OrderId` if any.
    /// Optimized to be fast for small number of active limit orders.
    #[inline(always)]
    #[must_use]
    pub fn get_by_id(
        &self,
        order_id: OrderId,
        side: Side,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        match side {
            Buy => self.bids.get_by_id(order_id),
            Sell => self.asks.get_by_id(order_id),
        }
    }

    /// Remove an active `LimitOrder` based on its order id.
    #[inline(always)]
    #[must_use]
    fn remove_by_id(
        &mut self,
        id: OrderId,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        if let Some(order) = self.bids.remove_by_id(id) {
            return Some(order);
        }
        if let Some(order) = self.asks.remove_by_id(id) {
            return Some(order);
        };
        None
    }

    /// Remove an active `LimitOrder` based on its order id.
    #[inline]
    fn remove_by_user_id(
        &mut self,
        uid: UserOrderIdT,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        if let Some(order) = self.bids.remove_by_user_id(uid) {
            return Some(order);
        }
        if let Some(order) = self.asks.remove_by_user_id(uid) {
            return Some(order);
        };
        None
    }

    /// Remove an order from being tracked for margin purposes.
    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    pub(crate) fn remove_limit_order(
        &mut self,
        by: CancelBy<UserOrderIdT>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        OrderIdNotFound<UserOrderIdT>,
    > {
        debug!("remove_limit_order {by:?}");
        use CancelBy::*;
        match by {
            OrderId(order_id) => self
                .remove_by_id(order_id)
                .ok_or(OrderIdNotFound::OrderId(order_id)),
            UserOrderId(user_order_id) => self
                .remove_by_user_id(user_order_id)
                .ok_or(OrderIdNotFound::UserOrderId(user_order_id)),
        }
    }

    /// fill an existing limit order, reduces order margin.
    #[inline]
    pub(crate) fn fill_best(
        &mut self,
        side: Side,
        filled_quantity: BaseOrQuote,
        ts_ns: TimestampNs,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>> {
        // TODO: deduplicate code with a macro or with new `SortedOrders` idea
        match side {
            Buy => self.bids.fill_best(filled_quantity, ts_ns),
            Sell => self.asks.fill_best(filled_quantity, ts_ns),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use const_decimal::Decimal;
    use num::{
        One,
        Zero,
    };
    use rand::Rng;

    use super::ActiveLimitOrders;
    use crate::{
        DECIMALS,
        prelude::{
            Position::{
                self,
                *,
            },
            PositionInner,
        },
        types::{
            BaseCurrency,
            CancelBy,
            Currency,
            ExchangeOrderMeta,
            Leverage,
            LimitOrder,
            QuoteCurrency,
            Side::{
                self,
                *,
            },
            TimestampNs,
        },
        utils::NoUserOrderId,
    };

    #[test]
    fn active_limit_orders_remove_by_id() {
        let mut book =
            ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>::with_capacity(
                NonZeroU16::new(10).unwrap(),
            );

        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.get_by_id(0.into(), Buy), Some(&order));
        assert_eq!(book.remove_by_id(0.into()), Some(order));
        assert_eq!(book.get_by_id(0.into(), Buy), None);
        assert_eq!(book.remove_by_id(0.into()), None);

        let order = LimitOrder::new(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.get_by_id(1.into(), Sell), Some(&order));
        assert_eq!(book.remove_by_id(1.into()), Some(order));
        assert_eq!(book.get_by_id(1.into(), Sell), None);
        assert_eq!(book.remove_by_id(1.into()), None);
    }

    #[test]
    fn active_limit_orders_remove_by_user_order_id() {
        let mut book = ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, i32>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let order = LimitOrder::new_with_user_order_id(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
            100,
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.remove_by_user_id(100), Some(order));
        assert_eq!(book.remove_by_user_id(100), None);

        let order = LimitOrder::new_with_user_order_id(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
            200,
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.remove_by_user_id(200), Some(order));
        assert_eq!(book.remove_by_user_id(200), None);
    }

    #[test]
    fn active_limit_orders_get_by_id() {
        let mut book =
            ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>::with_capacity(
                NonZeroU16::new(10).unwrap(),
            );
        for i in 0..10 {
            assert_eq!(book.get_by_id(i.into(), Buy), None);
        }
        for i in 0..10 {
            assert_eq!(book.get_by_id(i.into(), Sell), None);
        }

        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.get_by_id(0.into(), Buy).unwrap(), &order);
        for i in 1..10 {
            assert_eq!(book.get_by_id(i.into(), Buy), None);
        }

        let order = LimitOrder::new(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.get_by_id(0.into(), Sell).unwrap(), &order);
        for i in 1..10 {
            assert!(book.get_by_id(i.into(), Buy).is_none());
        }
    }

    #[test]
    #[tracing_test::traced_test]
    fn active_limit_orders_insert() {
        let mut book = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );
        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone()).unwrap();

        assert_eq!(book.num_active(), 1);
        let removed = book.remove_by_id(0.into()).unwrap();
        assert_eq!(removed, order);
        assert!(book.is_empty());

        let order_1 = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(200, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 1.into());
        let order_1 = order_1.into_pending(meta);
        book.try_insert(order_1.clone()).unwrap();
        assert_eq!(book.num_active(), 1);
        let removed = book.remove_by_id(1.into()).unwrap();
        assert_eq!(removed, order_1);
        assert!(book.is_empty());

        let mut rng = rand::rng();
        for i in 2..7 {
            let order = LimitOrder::new(
                Buy,
                QuoteCurrency::<i64, 5>::new(rng.random_range(100..500), 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap();
            let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
            let order = order.into_pending(meta);
            book.try_insert(order.clone()).unwrap();
        }
        assert_eq!(book.num_active(), 5);

        for i in 0..5 {
            let order = LimitOrder::new(
                Sell,
                QuoteCurrency::<i64, 5>::new(rng.random_range(100..500), 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap();
            let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
            let order = order.into_pending(meta);
            book.try_insert(order.clone()).unwrap();
        }
        assert_eq!(book.num_active(), 10);
    }

    #[test]
    fn order_margin_assert_limit_order_reduces_qty() {
        let new_active_order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let active_order = new_active_order.into_pending(meta);

        let mut updated_order = active_order.clone();
        updated_order.fill(BaseCurrency::new(1, 0));

        assert!(updated_order.remaining_quantity() < active_order.remaining_quantity());
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_no_orders(leverage: u8) {
        let order_margin = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let position = Position::<_, 4, BaseCurrency<i32, 4>>::Neutral;
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_long_no_orders(leverage: u8, position_qty: i32, entry_price: i32) {
        let order_margin = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let position = Long(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_short_no_orders(leverage: u8, position_qty: i32, entry_price: i32) {
        let order_margin = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let position = Short(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy, Sell],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_same_side(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        n: usize,
    ) {
        let max_active_orders = NonZeroU16::new(10).unwrap();
        let mut book =
            ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(max_active_orders);

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        let position = Neutral;
        orders
            .iter()
            .for_each(|order| book.try_insert(order.clone()).unwrap());

        let mult = QuoteCurrency::new(n as i32, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        orders.iter().for_each(|order| {
            let _ = book.remove_limit_order(CancelBy::OrderId(order.id()));
        });
        let om = QuoteCurrency::new(0, 0);
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_opposite_side(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        n: usize,
    ) {
        let mut book = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        let position = Neutral;
        buy_orders.iter().for_each(|order| {
            book.try_insert(order.clone()).unwrap();
        });
        let notional: QuoteCurrency<i32, 4> = buy_orders.iter().map(|o| o.notional()).sum();
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            notional * init_margin_req
        );

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            book.try_insert(order.clone()).unwrap();
        });

        let mult = QuoteCurrency::new(n as i32, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral,
            ),
            om
        );

        buy_orders.iter().for_each(|order| {
            let _ = book.remove_limit_order(CancelBy::OrderId(order.id()));
        });
        sell_orders.iter().for_each(|order| {
            let _ = book.remove_limit_order(CancelBy::OrderId(order.id()));
        });
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_long_orders_of_same_qty(leverage: u8) {
        let mut book = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(3, 0);
        let limit_price = QuoteCurrency::new(100, 0);

        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();

        let pos_entry_price = QuoteCurrency::new(90, 0);
        let position = Short(PositionInner::new(qty, pos_entry_price));

        // The limit orders may require more margin.
        let om = QuoteCurrency::convert_from(qty, QuoteCurrency::new(10, 0)) * init_margin_req;

        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy, Sell],
        [70, 90, 110],
        [1, 2, 3]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_update_partial_fills(
        leverage: u8,
        side: Side,
        limit_price: i64,
        qty: i64,
    ) {
        let mut book = ActiveLimitOrders::<_, DECIMALS, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        let notional = order.notional();
        let position = Neutral;

        book.try_insert(order.clone()).unwrap();
        assert_eq!(book.num_active(), 1);
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            notional * init_margin_req
        );

        // Now partially fill the order
        let filled_qty = qty / BaseCurrency::new(2, 0);
        order.fill(filled_qty);
        let remaining_qty = order.remaining_quantity();
        assert!(
            book.fill_best(order.side(), order.filled_quantity(), 0.into())
                .is_none()
        );
        assert_eq!(book.num_active(), 1);
        assert_eq!(remaining_qty, filled_qty);
        let om = QuoteCurrency::convert_from(remaining_qty, limit_price) * init_margin_req;
        assert_eq!(book.order_margin(init_margin_req, &Neutral), om,);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Neutral;
        let init_margin_req = Decimal::one();
        let mut book = ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap());

        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let qty = BaseCurrency::<i32, 4>::new(1, 0);
        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);

        book.try_insert(order).unwrap();
        assert_eq!(book.asks().len(), 0);
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.asks().len(), 1);
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.asks().len(), 2);
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut book = ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap());

        let qty = BaseCurrency::<i64, 5>::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Long(PositionInner::new(qty, entry_price));
        let init_margin_req = Decimal::one();

        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om,);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(120, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(185, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut book = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let qty = BaseCurrency::<i64, DECIMALS>::one();
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Short(PositionInner::new(qty, entry_price));
        let init_margin_req = Decimal::one();

        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::one();
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::zero()
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order).unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }
}
