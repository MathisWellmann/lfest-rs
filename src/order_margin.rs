use fpdec::{Dec, Decimal};
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
    pub(crate) fn update_order(
        &mut self,
        order: LimitOrder<Q, UserOrderId, Pending<Q>>,
        maker_fee: Fee,
    ) {
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

/// Compute the current order margin requirement, offset by the existing position if any.
#[allow(unused)] // The reference algorithm that `OrderMarginOnline` uses.
#[deprecated]
pub(crate) fn compute_order_margin_from_active_orders<Q, UserOrderId>(
    position: &Position<Q>,
    position_margin: Q::PairedCurrency,
    active_limit_orders: &ActiveLimitOrders<Q, UserOrderId>,
    init_margin_req: Decimal,
) -> Q::PairedCurrency
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone,
{
    // TODO: create a new type for `init_margin_req`.
    assert!(init_margin_req > Dec!(0));

    let (buy_margin_req, sell_margin_req) = active_limit_orders.values().fold(
        (Q::PairedCurrency::new_zero(), Q::PairedCurrency::new_zero()),
        |(b, s), order| {
            let notional_value = order.remaining_quantity().convert(order.limit_price());
            let margin_req = notional_value * init_margin_req;
            assert!(margin_req > Q::PairedCurrency::new_zero());
            match order.side() {
                Side::Buy => (b + margin_req, s),
                Side::Sell => (b, s + margin_req),
            }
        },
    );

    match position {
        Position::Neutral => max(buy_margin_req, sell_margin_req),
        Position::Long(_) => max(buy_margin_req, sell_margin_req - position_margin),
        Position::Short(_) => max(buy_margin_req - position_margin, sell_margin_req),
    }
}

#[cfg(test)]
mod tests {
    use hashbrown::HashMap;

    use super::*;
    use crate::prelude::*;

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let position_margin = quote!(0);
        let mut active_limit_orders = HashMap::default();
        let init_margin_req = Dec!(1);

        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(100)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(220)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1);
        let position = Position::Long(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let position_margin = quote!(100);
        let mut active_limit_orders = HashMap::default();
        let init_margin_req = Dec!(1);

        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(120)
        );

        let order = LimitOrder::new(Side::Buy, quote!(95), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(3, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(185)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1);
        let position = Position::Short(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let position_margin = quote!(100);
        let mut active_limit_orders = HashMap::default();
        let init_margin_req = Dec!(1);

        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(100)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(220)
        );

        let order = LimitOrder::new(Side::Buy, quote!(95), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(3, 0);
        let order = order.into_pending(meta);
        let order_id = order.state().meta().id();
        active_limit_orders.insert(order_id, order);
        assert_eq!(
            compute_order_margin_from_active_orders(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(220)
        );
    }
}
