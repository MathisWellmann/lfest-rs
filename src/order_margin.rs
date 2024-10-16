use std::ops::Neg;

use getset::{CopyGetters, Getters};
use num_traits::{One, Zero};
use tracing::trace;

use crate::{
    exchange::ActiveLimitOrders,
    prelude::{BasisPointFrac, CurrencyMarker, Mon, OrderId, Position},
    types::{LimitOrder, MarginCurrencyMarker, Pending, Side},
    utils::{max, min},
};

/// An implementation for computing the order margin online, aka with every change to the active orders.
#[derive(Debug, Clone, Default, CopyGetters, Getters)]
pub(crate) struct OrderMargin<I, const DB: u8, const DQ: u8, BaseOrQuote, UserOrderId>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
    UserOrderId: Clone + Default,
{
    #[getset(get = "pub(crate)")]
    active_limit_orders: ActiveLimitOrders<I, DB, DQ, BaseOrQuote, UserOrderId>,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote, UserOrderId>
    OrderMargin<I, DB, DQ, BaseOrQuote, UserOrderId>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<I, DB, DQ>,
    UserOrderId: Clone + std::fmt::Debug + std::cmp::PartialEq + Default,
{
    pub(crate) fn update(
        &mut self,
        order: &LimitOrder<I, DB, DQ, BaseOrQuote, UserOrderId, Pending<I, DB, DQ, BaseOrQuote>>,
    ) {
        assert!(order.remaining_quantity() > BaseOrQuote::zero());
        trace!("update_order: order: {order:?}");
        if let Some(active_order) = self.active_limit_orders.insert(order.id(), order.clone()) {
            assert_ne!(
                &active_order, order,
                "An update to an order should not be the same as the existing one"
            );
            assert!(order.remaining_quantity() < active_order.remaining_quantity(), "An update to an existing order must mean the new order has less quantity than the tracked order.");
            debug_assert_eq!(order.id(), active_order.id());

            // when an existing limit order is updated for margin purposes here, its quantity is always reduced.
            let removed_qty = active_order.remaining_quantity() - order.remaining_quantity();
            assert!(removed_qty > BaseOrQuote::zero());
        }
    }

    /// Remove an order from being tracked for margin purposes.
    pub(crate) fn remove(&mut self, order_id: OrderId) {
        self.active_limit_orders
            .remove(&order_id)
            .expect("Its an internal method call; it must work");
    }

    /// The margin requirement for all the tracked orders.
    pub(crate) fn order_margin(
        &self,
        init_margin_req: BasisPointFrac,
        position: &Position<I, DB, DQ, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        Self::order_margin_internal(&self.active_limit_orders, init_margin_req, position)
    }

    /// The margin requirement for all the tracked orders.
    fn order_margin_internal(
        active_limit_orders: &ActiveLimitOrders<I, DB, DQ, BaseOrQuote, UserOrderId>,
        init_margin_req: BasisPointFrac,
        position: &Position<I, DB, DQ, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        debug_assert!(init_margin_req <= BasisPointFrac::one());
        trace!("order_margin_internal: position: {position:?}, active_limit_orders: {active_limit_orders:?}");

        let mut buy_orders = Vec::from_iter(
            active_limit_orders
                .values()
                .filter(|order| matches!(order.side(), Side::Buy))
                .map(|order| (order.limit_price(), order.remaining_quantity())),
        );
        buy_orders.sort_by_key(|order| order.0.neg());

        let mut sell_orders = Vec::from_iter(
            active_limit_orders
                .values()
                .filter(|order| matches!(order.side(), Side::Sell))
                .map(|order| (order.limit_price(), order.remaining_quantity())),
        );
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
                    trace!("sells order_qty: {}, outstanding_pos_qty: {outstanding_pos_qty} new_qty: {new_qty}", sell_orders[i].1);
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
                    trace!("buys order_qty: {}, outstanding_pos_qty: {outstanding_pos_qty} new_qty: {new_qty}", buy_orders[i].1);
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

        // max(buy_value, sell_value) * init_margin_req
        todo!()
    }

    /// Get the order margin if a new order were to be added.
    pub(crate) fn order_margin_with_order(
        &self,
        order: &LimitOrder<I, DB, DQ, BaseOrQuote, UserOrderId, Pending<I, DB, DQ, BaseOrQuote>>,
        init_margin_req: BasisPointFrac,
        position: &Position<I, DB, DQ, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        let mut active_orders = self.active_limit_orders.clone();
        assert!(active_orders.insert(order.id(), order.clone()).is_none());
        Self::order_margin_internal(&active_orders, init_margin_req, position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prelude::*, MockTransactionAccounting, TEST_FEE_MAKER};

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_neutral_no_orders(leverage: u8) {
        let order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let position = Position::<_, 4, 2, BaseCurrency<i32, 4, 2>>::Neutral;
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
        let order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let mut accounting = MockTransactionAccounting::default();
        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            QuoteCurrency::new(0, 0),
        ));

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
        let order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let mut accounting = MockTransactionAccounting::default();
        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let position = Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            QuoteCurrency::new(0, 0),
        ));

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
        let mut order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        orders.iter().for_each(|order| order_margin.update(&order));

        let mult = QuoteCurrency::new(n as _, 0);
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, 2, BaseCurrency<i32, 4, 2>>::Neutral
            ),
            mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req
        );

        orders
            .iter()
            .for_each(|order| order_margin.remove(order.id()));
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, 2, BaseCurrency<i32, 4, 2>>::Neutral
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
        let mut order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        buy_orders.iter().for_each(|order| {
            order_margin.update(&order);
        });

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            order_margin.update(&order);
        });

        let mult = QuoteCurrency::new(n as _, 0);
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, 2, BaseCurrency<i32, 4, 2>>::Neutral,
            ),
            mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req
        );

        buy_orders
            .iter()
            .for_each(|order| order_margin.remove(order.id()));
        sell_orders
            .iter()
            .for_each(|order| order_margin.remove(order.id()));
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, 2, BaseCurrency<i32, 4, 2>>::Neutral
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
    fn order_margin_long_orders_of_same_qty(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        pos_entry_price: i32,
    ) {
        let mut order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);

        let mut accounting = MockTransactionAccounting::default();
        let pos_entry_price = QuoteCurrency::new(pos_entry_price, 0);
        let fees = QuoteCurrency::convert_from(qty, limit_price);
        let position = match side {
            Side::Buy => Position::Short(PositionInner::new(
                qty,
                pos_entry_price,
                &mut accounting,
                init_margin_req,
                fees,
            )),
            Side::Sell => Position::Long(PositionInner::new(
                qty,
                pos_entry_price,
                &mut accounting,
                init_margin_req,
                fees,
            )),
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
        limit_price: i32,
        qty: i32,
    ) {
        let mut order_margin = OrderMargin::<_, 4, 2, _, ()>::default();

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        order_margin.update(&order);

        // Now partially fill the order
        let filled_qty = qty / BaseCurrency::new(2, 0);
        assert!(order.fill(filled_qty, 0.into()).is_none());
        order_margin.update(&order);

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
        let init_margin_req = BasisPointFrac::one();
        let mut order_margin = OrderMargin::default();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let qty = BaseCurrency::<i32, 4, 2>::new(1, 0);
        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(100, 0)
        );

        let limit_price = QuoteCurrency::new(120, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(220, 0)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut accounting =
            InMemoryTransactionAccounting::new(QuoteCurrency::<i32, 4, 2>::new(1000, 0));
        let mut order_margin = OrderMargin::default();
        let init_margin_req = BasisPointFrac::one();

        let qty = BaseCurrency::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let fee = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee,
        ));
        let init_margin_req = BasisPointFrac::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(120, 0)
        );

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(185, 0)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut accounting = InMemoryTransactionAccounting::new(QuoteCurrency::new(1000, 0));
        let mut order_margin = OrderMargin::default();
        let init_margin_req = BasisPointFrac::one();

        let qty = BaseCurrency::<i32, 4, 2>::one();
        let entry_price = QuoteCurrency::new(100, 0);
        let fee = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let position = Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee,
        ));
        let init_margin_req = BasisPointFrac::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::one();
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::zero()
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(100, 0)
        );

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(220, 0)
        );

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(220, 0)
        );
    }
}
