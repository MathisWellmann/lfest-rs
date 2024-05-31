use fpdec::Decimal;
use getset::CopyGetters;
use tracing::trace;

use crate::{
    exchange::ActiveLimitOrders,
    prelude::Position,
    types::{Currency, Fee, LimitOrder, MarginCurrency, Pending, Side},
    utils::max,
};

/// An implementation for computing the order margin online, aka with every change to the active orders.
#[derive(Debug, Clone, Default, CopyGetters)]
pub(crate) struct OrderMarginOnline<Q, UserOrderId>
where
    Q: Currency,
    UserOrderId: Clone + Default,
{
    cumulative_buy_value: Q::PairedCurrency,
    cumulative_sell_value: Q::PairedCurrency,
    #[getset(get_copy = "pub(crate)")]
    cumulative_order_fees: Q::PairedCurrency,
    active_limit_orders: ActiveLimitOrders<Q, UserOrderId>,
}

impl<Q, UserOrderId> OrderMarginOnline<Q, UserOrderId>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + std::cmp::PartialEq + Default,
{
    pub(crate) fn update(&mut self, order: LimitOrder<Q, UserOrderId, Pending<Q>>, maker_fee: Fee) {
        trace!("update_order: order: {order:?}");
        if let Some(active_order) = self.active_limit_orders.get(&order.id()) {
            assert_ne!(active_order, &order);
            assert!(order.remaining_quantity() < active_order.remaining_quantity(), "An update to an existing order must mean the new order has less quantity than the tracked order.");
            assert_eq!(order.id(), active_order.id());

            let qty_delta: Q = active_order.remaining_quantity() - order.remaining_quantity();
            assert!(qty_delta > Q::new_zero());

            let notional_delta = qty_delta.convert(order.limit_price());

            match order.side() {
                Side::Buy => self.cumulative_buy_value -= notional_delta,
                Side::Sell => self.cumulative_sell_value -= notional_delta,
            }
            self.cumulative_order_fees -= notional_delta * maker_fee;
        } else {
            let notional_value = order.remaining_quantity().convert(order.limit_price());
            match order.side() {
                Side::Buy => self.cumulative_buy_value += notional_value,
                Side::Sell => self.cumulative_sell_value += notional_value,
            }
            self.active_limit_orders.insert(order.id(), order);
            self.cumulative_order_fees += notional_value * maker_fee;
        }
    }

    /// Remove an order from being tracked for margin purposes.
    pub(crate) fn remove_order(
        &mut self,
        order: &LimitOrder<Q, UserOrderId, Pending<Q>>,
        maker_fee: Fee,
    ) {
        let removed_order = self
            .active_limit_orders
            .remove(&order.id())
            .expect("Its an internal method call; it must work");

        let notional_value = removed_order
            .remaining_quantity()
            .convert(removed_order.limit_price());
        match order.side() {
            Side::Buy => {
                self.cumulative_buy_value -= notional_value;
                assert!(self.cumulative_buy_value >= Q::PairedCurrency::new_zero());
            }

            Side::Sell => {
                self.cumulative_sell_value -= notional_value;
                assert!(self.cumulative_sell_value >= Q::PairedCurrency::new_zero());
            }
        }
        let fee = notional_value * maker_fee;
        self.cumulative_order_fees -= fee;
    }

    /// The margin requirement for all the tracked orders.
    pub(crate) fn order_margin(
        &self,
        init_margin_req: Decimal,
        position: &Position<Q>,
        position_margin: Q::PairedCurrency,
    ) -> Q::PairedCurrency {
        let buy_margin_req = self.cumulative_buy_value * init_margin_req;
        let sell_margin_req = self.cumulative_sell_value * init_margin_req;
        match position {
            Position::Neutral => max(buy_margin_req, sell_margin_req),
            Position::Long(_) => max(buy_margin_req, sell_margin_req - position_margin),
            Position::Short(_) => max(buy_margin_req - position_margin, sell_margin_req),
        }
    }

    /// Get the order margin if a new order were to be added.
    pub(crate) fn order_margin_with_order(
        &self,
        order: &LimitOrder<Q, UserOrderId, Pending<Q>>,
        init_margin_req: Decimal,
        position: &Position<Q>,
        position_margin: Q::PairedCurrency,
    ) -> Q::PairedCurrency {
        let notional_value = order.remaining_quantity().convert(order.limit_price());
        let margin_req = notional_value * init_margin_req;

        let mut buy_margin_req = self.cumulative_buy_value * init_margin_req;
        let mut sell_margin_req = self.cumulative_sell_value * init_margin_req;

        match order.side() {
            Side::Buy => buy_margin_req += margin_req,
            Side::Sell => sell_margin_req += margin_req,
        }

        match position {
            Position::Neutral => max(buy_margin_req, sell_margin_req),
            Position::Long(_) => max(buy_margin_req, sell_margin_req - position_margin),
            Position::Short(_) => max(buy_margin_req - position_margin, sell_margin_req),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prelude::*, utils::f64_to_decimal, MockTransactionAccounting};

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_online_no_orders_neutral(leverage: u32) {
        let order_margin = OrderMarginOnline::<_, ()>::default();

        let init_margin_req = f64_to_decimal(leverage as f64, Dec!(0.01));

        let position = Position::<BaseCurrency>::Neutral;
        let position_margin = quote!(0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_online_no_orders_long(leverage: u32, position_qty: u32, entry_price: u32) {
        let order_margin = OrderMarginOnline::<_, ()>::default();

        let mut accounting = MockTransactionAccounting::default();
        let qty = BaseCurrency::new(Decimal::from(position_qty));
        let entry_price = QuoteCurrency::new(Decimal::from(entry_price));
        let init_margin_req = f64_to_decimal(leverage as f64, Dec!(0.01));
        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
        ));
        let position_margin = qty.convert(entry_price) * init_margin_req;

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_online_no_orders_short(leverage: u32, position_qty: u32, entry_price: u32) {
        let order_margin = OrderMarginOnline::<_, ()>::default();

        let mut accounting = MockTransactionAccounting::default();
        let qty = BaseCurrency::new(Decimal::from(position_qty));
        let entry_price = QuoteCurrency::new(Decimal::from(entry_price));
        let init_margin_req = f64_to_decimal(leverage as f64, Dec!(0.01));
        let position = Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
        ));
        let position_margin = qty.convert(entry_price) * init_margin_req;

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );
        assert_eq!(order_margin.cumulative_order_fees(), quote!(0));
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [100, 150, 200],
        [1, 2, 3]
    )]
    fn order_margin_neutral_one_order(leverage: u32, side: Side, limit_price: u32, qty: u32) {
        let mut order_margin = OrderMarginOnline::<_, ()>::default();

        let init_margin_req = f64_to_decimal(leverage as f64, Dec!(0.01));
        let fee_maker = fee!(0.0002);

        let qty = BaseCurrency::new(Decimal::from(qty));
        let limit_price = QuoteCurrency::new(Decimal::from(limit_price));
        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::default();
        let order = order.into_pending(meta);

        order_margin.update(order, fee_maker);

        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<BaseCurrency>::Neutral,
                quote!(0)
            ),
            qty.convert(limit_price) * init_margin_req
        );
        assert_eq!(
            order_margin.cumulative_order_fees(),
            qty.convert(limit_price) * fee_maker
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let position_margin = quote!(0);
        let init_margin_req = Dec!(1);
        let fee_maker = fee!(0.0002);
        let mut order_margin = OrderMarginOnline::default();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(100)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(220)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let mut order_margin = OrderMarginOnline::default();
        let init_margin_req = Dec!(1);
        let fee_maker = fee!(0.0002);
        let position = Position::Long(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let position_margin = quote!(100);
        let init_margin_req = Dec!(1);

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(120)
        );

        let order = LimitOrder::new(Side::Buy, quote!(95), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(3, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(185)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let mut order_margin = OrderMarginOnline::default();
        let init_margin_req = Dec!(1);
        let fee_maker = fee!(0.0002);
        let position = Position::Short(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let position_margin = quote!(100);
        let init_margin_req = Dec!(1);

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(100)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(220)
        );

        let order = LimitOrder::new(Side::Buy, quote!(95), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(3, 0);
        let order = order.into_pending(meta);
        order_margin.update(order, fee_maker);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position, position_margin),
            quote!(220)
        );
    }
}
