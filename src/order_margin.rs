use fpdec::{Dec, Decimal};

use crate::{
    exchange::ActiveLimitOrders,
    prelude::Position,
    types::{Currency, MarginCurrency, Side},
    utils::max,
};

/// Compute the current order margin requirement, offset by the existing position if any.
pub(crate) fn compute_order_margin<Q, UserOrderId>(
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
    use crate::{position::PositionInner, prelude::*};

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let position_margin = quote!(0);
        let mut active_limit_orders = HashMap::default();
        let init_margin_req = Dec!(1);

        assert_eq!(
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
        let position = Position::Long(PositionInner::new(base!(1), quote!(100)));
        let position_margin = quote!(100);
        let mut active_limit_orders = HashMap::default();
        let init_margin_req = Dec!(1);

        assert_eq!(
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
        let position = Position::Short(PositionInner::new(base!(1), quote!(100)));
        let position_margin = quote!(100);
        let mut active_limit_orders = HashMap::default();
        let init_margin_req = Dec!(1);

        assert_eq!(
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
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
            compute_order_margin(
                &position,
                position_margin,
                &active_limit_orders,
                init_margin_req,
            ),
            quote!(220)
        );
    }
}
