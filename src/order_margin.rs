use std::ops::Neg;

use const_decimal::Decimal;
use getset::{CopyGetters, Getters};
use num_traits::{One, Zero};
use tracing::trace;

use crate::{
    Result,
    exchange::CancelBy,
    prelude::{ActiveLimitOrders, Currency, Mon, Position},
    types::{LimitOrder, MarginCurrency, Pending, Side, UserOrderId},
    utils::{max, min},
};

/// An implementation for computing the order margin online, aka with every change to the active orders.
#[derive(Debug, Clone, CopyGetters, Getters)]
pub(crate) struct OrderMargin<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[getset(get = "pub(crate)")]
    active_limit_orders: ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> OrderMargin<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    pub(crate) fn new(max_active_orders: usize) -> Self {
        Self {
            active_limit_orders: ActiveLimitOrders::new(max_active_orders),
        }
    }

    pub(crate) fn update(
        &mut self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<()> {
        assert!(order.remaining_quantity() > BaseOrQuote::zero());
        trace!("update_order: order: {order:?}");
        if let Some(active_order) = self.active_limit_orders.insert(order.clone())? {
            assert_ne!(
                &active_order, order,
                "An update to an order should not be the same as the existing one"
            );
            assert!(
                order.remaining_quantity() < active_order.remaining_quantity(),
                "An update to an existing order must mean the new order has less quantity than the tracked order."
            );
            debug_assert_eq!(order.id(), active_order.id());
            Self::assert_limit_order_update_reduces_qty(&active_order, order);
        }
        Ok(())
    }

    fn assert_limit_order_update_reduces_qty(
        active_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        updated_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) {
        // when an existing limit order is updated for margin purposes here, its quantity is always reduced.
        let removed_qty = active_order.remaining_quantity() - updated_order.remaining_quantity();
        assert!(removed_qty > BaseOrQuote::zero());
    }

    /// Remove an order from being tracked for margin purposes.
    pub(crate) fn remove(&mut self, by: CancelBy<UserOrderIdT>) {
        match by {
            CancelBy::OrderId(order_id) => {
                let _ = self
                    .active_limit_orders
                    .remove_by_order_id(order_id)
                    .expect("Its an internal method call; it must work");
            }
            CancelBy::UserOrderId(user_order_id) => {
                let _ = self
                    .active_limit_orders
                    .remove_by_user_order_id(user_order_id)
                    .expect("Its an internal method call; it must work");
            }
        }
    }

    /// The margin requirement for all the tracked orders.
    pub(crate) fn order_margin(
        &self,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        Self::order_margin_internal(&self.active_limit_orders, init_margin_req, position, None)
    }

    /// The margin requirement for all the tracked orders.
    fn order_margin_internal(
        active_limit_orders: &ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
        opt_new_order: Option<
            &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        >,
    ) -> BaseOrQuote::PairedCurrency {
        debug_assert!(init_margin_req > Decimal::zero());
        debug_assert!(init_margin_req <= Decimal::one());

        trace!(
            "order_margin_internal: position: {position:?}, active_limit_orders: {active_limit_orders:?}"
        );

        let mut buy_orders = Vec::from_iter(
            active_limit_orders
                .values()
                .filter(|order| matches!(order.side(), Side::Buy))
                .map(|order| (order.limit_price(), order.remaining_quantity())),
        );

        let mut sell_orders = Vec::from_iter(
            active_limit_orders
                .values()
                .filter(|order| matches!(order.side(), Side::Sell))
                .map(|order| (order.limit_price(), order.remaining_quantity())),
        );
        if let Some(new_order) = opt_new_order {
            match new_order.side() {
                Side::Buy => {
                    buy_orders.push((new_order.limit_price(), new_order.remaining_quantity()))
                }
                Side::Sell => {
                    sell_orders.push((new_order.limit_price(), new_order.remaining_quantity()))
                }
            }
        }
        buy_orders.sort_by_key(|order| order.0.neg());
        sell_orders.sort_by_key(|order| order.0);

        match position {
            Position::Neutral => {}
            Position::Long(inner) => {
                let mut outstanding_pos_qty = inner.quantity();
                let mut i = 0;
                while outstanding_pos_qty > BaseOrQuote::zero() {
                    if i >= sell_orders.len() {
                        break;
                    }
                    let new_qty = max(sell_orders[i].1 - outstanding_pos_qty, BaseOrQuote::zero());
                    trace!(
                        "sells order_qty: {}, outstanding_pos_qty: {outstanding_pos_qty} new_qty: {new_qty}",
                        sell_orders[i].1
                    );
                    outstanding_pos_qty -= min(sell_orders[i].1, outstanding_pos_qty);
                    sell_orders[i].1 = new_qty;
                    i += 1;
                }
            }
            Position::Short(inner) => {
                let mut outstanding_pos_qty = inner.quantity();
                let mut i = 0;
                while outstanding_pos_qty > BaseOrQuote::zero() {
                    if i >= buy_orders.len() {
                        break;
                    }
                    let new_qty = max(buy_orders[i].1 - outstanding_pos_qty, BaseOrQuote::zero());
                    trace!(
                        "buys order_qty: {}, outstanding_pos_qty: {outstanding_pos_qty} new_qty: {new_qty}",
                        buy_orders[i].1
                    );
                    outstanding_pos_qty -= min(buy_orders[i].1, outstanding_pos_qty);
                    buy_orders[i].1 = new_qty;
                    i += 1;
                }
            }
        }

        let mut buy_value = BaseOrQuote::PairedCurrency::zero();
        buy_orders.iter().for_each(|(price, qty)| {
            buy_value += BaseOrQuote::PairedCurrency::convert_from(*qty, *price)
        });

        let mut sell_value = BaseOrQuote::PairedCurrency::zero();
        sell_orders.iter().for_each(|(price, qty)| {
            sell_value += BaseOrQuote::PairedCurrency::convert_from(*qty, *price)
        });

        let om = max(buy_value, sell_value) * init_margin_req;
        trace!("required order_margin: {om}");
        om
    }

    /// Get the order margin if a new order were to be added.
    pub(crate) fn order_margin_with_order(
        &self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        Self::order_margin_internal(
            &self.active_limit_orders,
            init_margin_req,
            position,
            Some(order),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DECIMALS, prelude::*, test_fee_maker};

    #[test]
    fn order_margin_assert_limit_order_reduces_qty() {
        let new_active_order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let active_order = new_active_order.into_pending(meta);

        let mut updated_order = active_order.clone();
        let fee = QuoteCurrency::new(0, 0);
        updated_order.fill(BaseCurrency::new(1, 0), fee, 1.into());

        OrderMargin::assert_limit_order_update_reduces_qty(&active_order, &updated_order);
    }

    #[test]
    #[should_panic]
    fn order_margin_assert_limit_order_reduces_qty_panic() {
        let new_active_order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let order_0 = new_active_order.into_pending(meta);

        let mut order_1 = order_0.clone();
        let fee = QuoteCurrency::new(0, 0);
        order_1.fill(BaseCurrency::new(1, 0), fee, 1.into());

        OrderMargin::assert_limit_order_update_reduces_qty(&order_1, &order_0);
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_no_orders(leverage: u8) {
        let order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(10);

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
        let order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(10);

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(1500, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Position::Long(PositionInner::new(qty, entry_price));

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
        let order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(10);

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Position::Short(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
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
        let mut order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(10);

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        orders
            .iter()
            .for_each(|order| order_margin.update(&order).unwrap());

        let mult = QuoteCurrency::new(n as _, 0);
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req
        );

        orders
            .iter()
            .for_each(|order| order_margin.remove(CancelBy::OrderId(order.id())));
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy],
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
        let mut order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(10);

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        buy_orders.iter().for_each(|order| {
            order_margin.update(&order).unwrap();
        });

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            order_margin.update(&order).unwrap();
        });

        let mult = QuoteCurrency::new(n as _, 0);
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral,
            ),
            mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req
        );

        buy_orders
            .iter()
            .for_each(|order| order_margin.remove(CancelBy::OrderId(order.id())));
        sell_orders
            .iter()
            .for_each(|order| order_margin.remove(CancelBy::OrderId(order.id())));
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            QuoteCurrency::new(0, 0)
        );
    }

    /// The position always cancels out the orders, so the order margin is zero.
    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [70, 90, 110],
        [1, 2, 3],
        [85, 100, 125]
    )]
    #[tracing_test::traced_test]
    fn order_margin_long_orders_of_same_qty(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        pos_entry_price: i32,
    ) {
        let mut order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(10);

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();

        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();

        let pos_entry_price = QuoteCurrency::new(pos_entry_price, 0);

        let position = match side {
            Side::Buy => Position::Short(PositionInner::new(qty, pos_entry_price)),
            Side::Sell => Position::Long(PositionInner::new(qty, pos_entry_price)),
        };

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0),
            "The position quantity always cancels out the limit orders. So margin requirement is 0."
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [70, 90, 110],
        [1, 2, 3]
    )]
    fn order_margin_neutral_update_partial_fills(
        leverage: u8,
        side: Side,
        limit_price: i64,
        qty: i64,
    ) {
        let mut order_margin = OrderMargin::<_, DECIMALS, _, NoUserOrderId>::new(10);

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        order_margin.update(&order).unwrap();

        // Now partially fill the order
        let filled_qty = qty / BaseCurrency::new(2, 0);
        let fee = QuoteCurrency::convert_from(filled_qty, limit_price) * *test_fee_maker().as_ref();
        match order.fill(filled_qty, fee, 0.into()) {
            LimitOrderFill::PartiallyFilled {
                fill_price,
                filled_quantity,
                fee: f,
                order_after_fill: _,
            } => {
                assert_eq!(fill_price, limit_price);
                assert_eq!(filled_quantity, filled_qty);
                assert_eq!(f, fee);
            }
            LimitOrderFill::FullyFilled { .. } => panic!("Expected `PartiallyFilled`"),
        }
        order_margin.update(&order).unwrap();

        let remaining_qty = order.remaining_quantity();
        assert_eq!(remaining_qty, filled_qty);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &Position::Neutral),
            QuoteCurrency::convert_from(remaining_qty, limit_price) * init_margin_req
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let init_margin_req = Decimal::one();
        let mut order_margin = OrderMargin::new(10);

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let qty = BaseCurrency::<i32, 4>::new(1, 0);
        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(100, 0)
        );

        let limit_price = QuoteCurrency::new(120, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(220, 0)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut order_margin = OrderMargin::new(10);

        let qty = BaseCurrency::<i64, 5>::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Position::Long(PositionInner::new(qty, entry_price));
        let init_margin_req = Decimal::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(120, 0)
        );

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(185, 0)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut order_margin = OrderMargin::<i64, 5, _, NoUserOrderId>::new(10);

        let qty = BaseCurrency::<i64, DECIMALS>::one();
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Position::Short(PositionInner::new(qty, entry_price));
        let init_margin_req = Decimal::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::one();
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::zero()
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(100, 0)
        );

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(220, 0)
        );

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order).unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(220, 0)
        );
    }
}
