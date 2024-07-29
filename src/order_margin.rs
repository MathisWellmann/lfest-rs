use fpdec::{Dec, Decimal};
use getset::{CopyGetters, Getters};
use tracing::trace;

use crate::{
    exchange::ActiveLimitOrders,
    prelude::{OrderId, Position},
    types::{Currency, Fee, LimitOrder, MarginCurrency, Pending, Side},
    utils::{max, min},
};

/// An implementation for computing the order margin online, aka with every change to the active orders.
#[derive(Debug, Clone, Default, CopyGetters, Getters)]
pub(crate) struct OrderMargin<Q, UserOrderId>
where
    Q: Currency,
    UserOrderId: Clone + Default,
{
    #[getset(get_copy = "pub(crate)")]
    cumulative_order_fees: Q::PairedCurrency,
    #[getset(get = "pub(crate)")]
    active_limit_orders: ActiveLimitOrders<Q, UserOrderId>,
}

impl<Q, UserOrderId> OrderMargin<Q, UserOrderId>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + std::cmp::PartialEq + Default,
{
    pub(crate) fn update(
        &mut self,
        order: &LimitOrder<Q, UserOrderId, Pending<Q>>,
        fee_maker: Fee,
    ) {
        assert!(order.remaining_quantity() > Q::new_zero());
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
            assert!(removed_qty > Q::new_zero());
            self.cumulative_order_fees -= removed_qty.convert(order.limit_price()) * fee_maker;
            assert!(self.cumulative_order_fees >= Q::PairedCurrency::new_zero());
        } else {
            let notional_value = order.remaining_quantity().convert(order.limit_price());
            self.cumulative_order_fees += notional_value * fee_maker;
        }
    }

    /// Remove an order from being tracked for margin purposes.
    pub(crate) fn remove(&mut self, order_id: OrderId, fee_maker: Fee) {
        let removed_order = self
            .active_limit_orders
            .remove(&order_id)
            .expect("Its an internal method call; it must work");

        let notional_value = removed_order
            .remaining_quantity()
            .convert(removed_order.limit_price());
        self.cumulative_order_fees -= notional_value * fee_maker;
        assert2::assert!(self.cumulative_order_fees >= Q::PairedCurrency::new_zero());
    }

    /// The margin requirement for all the tracked orders.
    pub(crate) fn order_margin_with_fees(
        &self,
        init_margin_req: Decimal,
        position: &Position<Q>,
    ) -> Q::PairedCurrency {
        let om = Self::order_margin_internal(&self.active_limit_orders, init_margin_req, position);
        om + self.cumulative_order_fees
    }

    /// The margin requirement for all the tracked orders.
    fn order_margin_internal(
        active_limit_orders: &ActiveLimitOrders<Q, UserOrderId>,
        init_margin_req: Decimal,
        position: &Position<Q>,
    ) -> Q::PairedCurrency {
        debug_assert!(init_margin_req <= Dec!(1));
        trace!("order_margin_internal: position: {position:?}, active_limit_orders: {active_limit_orders:?}");

        let mut buy_orders = Vec::from_iter(
            active_limit_orders
                .values()
                .filter(|order| matches!(order.side(), Side::Buy))
                .map(|order| (order.limit_price(), order.remaining_quantity())),
        );
        buy_orders.sort_by_key(|order| order.0.into_negative());

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
                while outstanding_pos_qty > Q::new_zero() {
                    if i >= sell_orders.len() {
                        break;
                    }
                    let new_qty = max(sell_orders[i].1 - outstanding_pos_qty, Q::new_zero());
                    trace!("sells order_qty: {}, outstanding_pos_qty: {outstanding_pos_qty} new_qty: {new_qty}", sell_orders[i].1);
                    outstanding_pos_qty -= min(sell_orders[i].1, outstanding_pos_qty);
                    sell_orders[i].1 = new_qty;
                    i += 1;
                }
            }
            Position::Short(inner) => {
                let mut outstanding_pos_qty = inner.quantity();
                let mut i = 0;
                while outstanding_pos_qty > Q::new_zero() {
                    if i >= buy_orders.len() {
                        break;
                    }
                    let new_qty = max(buy_orders[i].1 - outstanding_pos_qty, Q::new_zero());
                    trace!("buys order_qty: {}, outstanding_pos_qty: {outstanding_pos_qty} new_qty: {new_qty}", buy_orders[i].1);
                    outstanding_pos_qty -= min(buy_orders[i].1, outstanding_pos_qty);
                    buy_orders[i].1 = new_qty;
                    i += 1;
                }
            }
        }

        let mut buy_value = Q::PairedCurrency::new_zero();
        buy_orders
            .iter()
            .for_each(|(price, qty)| buy_value += qty.convert(*price));

        let mut sell_value = Q::PairedCurrency::new_zero();
        sell_orders
            .iter()
            .for_each(|(price, qty)| sell_value += qty.convert(*price));

        max(buy_value, sell_value) * init_margin_req
    }

    /// Get the order margin if a new order were to be added.
    pub(crate) fn order_margin_and_fees_with_order(
        &self,
        order: &LimitOrder<Q, UserOrderId, Pending<Q>>,
        init_margin_req: Decimal,
        position: &Position<Q>,
    ) -> Q::PairedCurrency {
        let mut active_orders = self.active_limit_orders.clone();
        assert!(active_orders.insert(order.id(), order.clone()).is_none());
        let om = Self::order_margin_internal(&active_orders, init_margin_req, position);
        om + self.cumulative_order_fees
    }

    #[cfg(test)]
    pub(crate) fn from_parts(
        cumulative_order_fees: Q::PairedCurrency,
        active_limit_orders: ActiveLimitOrders<Q, UserOrderId>,
    ) -> Self {
        Self {
            cumulative_order_fees,
            active_limit_orders,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prelude::*, MockTransactionAccounting};

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_neutral_no_orders(leverage: u32) {
        let order_margin = OrderMargin::<_, ()>::default();

        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);

        let position = Position::<BaseCurrency>::Neutral;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_long_no_orders(leverage: u32, position_qty: u32, entry_price: u32) {
        let order_margin = OrderMargin::<_, ()>::default();

        let mut accounting = MockTransactionAccounting::default();
        let qty = BaseCurrency::new(Decimal::from(position_qty));
        let entry_price = QuoteCurrency::new(Decimal::from(entry_price));
        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);
        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
        ));

        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_short_no_orders(leverage: u32, position_qty: u32, entry_price: u32) {
        let order_margin = OrderMargin::<_, ()>::default();

        let mut accounting = MockTransactionAccounting::default();
        let qty = BaseCurrency::new(Decimal::from(position_qty));
        let entry_price = QuoteCurrency::new(Decimal::from(entry_price));
        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);
        let position = Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
        ));

        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_same_side(
        leverage: u32,
        side: Side,
        limit_price: u32,
        qty: u32,
        n: usize,
    ) {
        let mut order_margin = OrderMargin::<_, ()>::default();

        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);
        let fee_maker = fee!(0.0002);

        let qty = BaseCurrency::new(Decimal::from(qty));
        let limit_price = QuoteCurrency::new(Decimal::from(limit_price));

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        orders
            .iter()
            .for_each(|order| order_margin.update(&order, fee_maker));

        let mult = QuoteCurrency::new(Decimal::from(n as u64));
        let fees = (qty.convert(limit_price) * fee_maker) * mult;
        assert_eq!(
            order_margin
                .order_margin_with_fees(init_margin_req, &Position::<BaseCurrency>::Neutral,),
            mult * qty.convert(limit_price) * init_margin_req + fees
        );
        assert_eq!(
            order_margin.cumulative_order_fees(),
            mult * qty.convert(limit_price) * fee_maker
        );

        orders
            .iter()
            .for_each(|order| order_margin.remove(order.id(), fee_maker));
        assert_eq!(
            order_margin
                .order_margin_with_fees(init_margin_req, &Position::<BaseCurrency>::Neutral),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_opposite_side(
        leverage: u32,
        side: Side,
        limit_price: u32,
        qty: u32,
        n: usize,
    ) {
        let mut order_margin = OrderMargin::<_, ()>::default();

        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);
        let fee_maker = fee!(0.0002);

        let qty = BaseCurrency::new(Decimal::from(qty));
        let limit_price = QuoteCurrency::new(Decimal::from(limit_price));

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        buy_orders.iter().for_each(|order| {
            order_margin.update(&order, fee_maker);
        });

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            order_margin.update(&order, fee_maker);
        });

        let mult = QuoteCurrency::new(Decimal::from(n as u64));
        let fees = (qty.convert(limit_price) * fee_maker) * mult * Dec!(2);
        assert_eq!(
            order_margin
                .order_margin_with_fees(init_margin_req, &Position::<BaseCurrency>::Neutral,),
            mult * qty.convert(limit_price) * init_margin_req + fees
        );
        assert_eq!(
            order_margin.cumulative_order_fees(),
            quote!(2) * mult * qty.convert(limit_price) * fee_maker
        );

        buy_orders
            .iter()
            .for_each(|order| order_margin.remove(order.id(), fee_maker));
        sell_orders
            .iter()
            .for_each(|order| order_margin.remove(order.id(), fee_maker));
        assert_eq!(
            order_margin
                .order_margin_with_fees(init_margin_req, &Position::<BaseCurrency>::Neutral),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
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
        leverage: u32,
        side: Side,
        limit_price: u32,
        qty: u32,
        pos_entry_price: u32,
    ) {
        let mut order_margin = OrderMargin::<_, ()>::default();

        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);
        let fee_maker = fee!(0.0002);

        let qty = BaseCurrency::new(Decimal::from(qty));
        let limit_price = QuoteCurrency::new(Decimal::from(limit_price));

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);

        let mut accounting = MockTransactionAccounting::default();
        let pos_entry_price = QuoteCurrency::new(Decimal::from(pos_entry_price));
        let position = match side {
            Side::Buy => Position::Short(PositionInner::new(
                qty,
                pos_entry_price,
                &mut accounting,
                init_margin_req,
            )),
            Side::Sell => Position::Long(PositionInner::new(
                qty,
                pos_entry_price,
                &mut accounting,
                init_margin_req,
            )),
        };

        let fees = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            fees,
            "The position quantity always cancels out the limit orders. So margin requirement is 0, but with fees."
        );
        assert_eq!(
            order_margin.cumulative_order_fees(),
            qty.convert(limit_price) * fee_maker
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [70, 90, 110],
        [1, 2, 3]
    )]
    fn order_margin_neutral_update_partial_fills(
        leverage: u32,
        side: Side,
        limit_price: u32,
        qty: u32,
    ) {
        let mut order_margin = OrderMargin::<_, ()>::default();

        let init_margin_req = Dec!(1.0) / Decimal::from(leverage);
        let fee_maker = fee!(0.0002);

        let qty = BaseCurrency::new(Decimal::from(qty));
        let limit_price = QuoteCurrency::new(Decimal::from(limit_price));

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);

        // Now partially fill the order
        let filled_qty = qty / base!(2);
        assert!(order.fill(filled_qty, 0.into()).is_none());
        order_margin.update(&order, fee_maker);

        let remaining_qty = order.remaining_quantity();
        assert_eq!(remaining_qty, filled_qty);
        let fees = remaining_qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &Position::Neutral),
            remaining_qty.convert(limit_price) * init_margin_req + fees
        );
        assert_eq!(
            order_margin.cumulative_order_fees(),
            remaining_qty.convert(limit_price) * fee_maker
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let init_margin_req = Dec!(1);
        let fee_maker = fee!(0.0002);
        let mut order_margin = OrderMargin::default();

        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0)
        );

        let qty = base!(1);
        let limit_price = quote!(90);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_0 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(90) + fee_0
        );

        let limit_price = quote!(100);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_1 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(100) + fee_0 + fee_1
        );

        let limit_price = quote!(120);
        let qty = base!(1);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_2 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(220) + fee_0 + fee_1 + fee_2
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let mut order_margin = OrderMargin::default();
        let init_margin_req = Dec!(1);
        let fee_maker = fee!(0.0002);
        let position = Position::Long(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let init_margin_req = Dec!(1);

        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0)
        );

        let limit_price = quote!(90);
        let qty = base!(1);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_0 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(90) + fee_0
        );

        let limit_price = quote!(100);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_1 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(90) + fee_0 + fee_1
        );

        let limit_price = quote!(120);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_2 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(120) + fee_0 + fee_1 + fee_2
        );

        let limit_price = quote!(95);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_3 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(185) + fee_0 + fee_1 + fee_2 + fee_3
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let mut order_margin = OrderMargin::default();
        let init_margin_req = Dec!(1);
        let fee_maker = fee!(0.0002);
        let position = Position::Short(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let init_margin_req = Dec!(1);

        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0)
        );

        let limit_price = quote!(90);
        let qty = base!(1);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_0 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(0) + fee_0
        );

        let limit_price = quote!(100);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_1 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(100) + fee_0 + fee_1
        );

        let limit_price = quote!(120);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_2 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(220) + fee_0 + fee_1 + fee_2
        );

        let limit_price = quote!(95);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin.update(&order, fee_maker);
        let fee_3 = qty.convert(limit_price) * fee_maker;
        assert_eq!(
            order_margin.order_margin_with_fees(init_margin_req, &position),
            quote!(220) + fee_0 + fee_1 + fee_2 + fee_3
        );
    }
}
