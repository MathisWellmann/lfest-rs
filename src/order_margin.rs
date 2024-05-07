use hashbrown::HashMap;

use crate::{
    prelude::Position,
    types::{Currency, Leverage, LimitOrder, MarginCurrency, OrderId, Pending, Side},
    utils::{max, min},
};

/// Compute the current order margin requirement.
#[allow(clippy::type_complexity)]
pub(crate) fn compute_order_margin<M, UserOrderId>(
    position: &Position<M>,
    active_limit_orders: &HashMap<
        OrderId,
        LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
    >,
    leverage: Leverage,
) -> M
where
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug,
{
    // New Algo:
    // 1: Orders are split into buys and sells
    // 2: They are sorted by ascending price
    // 3: Each side is handled separately
    // 4: For buys:
    //  - An existing short position offsets the order size closest to the positions entry price
    //  - Anything that cannot be offset: notional value is accumulated
    // 5: For sells its the same but reversed.

    let mut buys = Vec::from_iter(
        active_limit_orders
            .values()
            .filter(|order| matches!(order.side(), Side::Buy)),
    );
    buys.sort_by_key(|order| order.limit_price());

    let mut sells = Vec::from_iter(
        active_limit_orders
            .values()
            .filter(|order| matches!(order.side(), Side::Sell)),
    );
    sells.sort_by_key(|order| order.limit_price());

    // accumulate notional value (+fee) of buy orders which are not offset by the position
    let mut buy_margin_req = M::new_zero();
    let mut remaining_short_size = min(position.size(), M::PairedCurrency::new_zero()).abs();
    for b in &buys {
        let mut order_qty = b.remaining_quantity();
        if remaining_short_size > M::PairedCurrency::new_zero() {
            // offset the order qty by as much as possible
            let offset = max(order_qty, remaining_short_size);
            order_qty -= offset;
            remaining_short_size -= offset;
        }
        let order_value = order_qty.convert(b.limit_price());
        let margin_req = order_value / leverage;
        buy_margin_req += margin_req;
    }

    let mut sell_margin_req = M::new_zero();
    let mut remaining_long_size = max(position.size(), M::PairedCurrency::new_zero());
    for s in &sells {
        let mut order_qty = s.remaining_quantity();
        if remaining_long_size > M::PairedCurrency::new_zero() {
            // offset the order qty by as much as possible
            let offset = max(order_qty, remaining_long_size);
            order_qty -= offset;
            remaining_long_size -= offset;
        }
        let order_value = order_qty.convert(s.limit_price());
        let margin_req = order_value / leverage;
        sell_margin_req += margin_req;
    }

    max(buy_margin_req, sell_margin_req)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let leverage = leverage!(1);
        let mut account = Account::new(quote!(1000), leverage, fee!(0.0002), fee!(0.0006));

        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage(),
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                &account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(100)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                &account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(220)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut account = Account::new(quote!(1000), leverage!(1), fee!(0.0002), fee!(0.0006));
        *account.position_mut() = Position {
            size: base!(1),
            entry_price: quote!(100),
            margin: quote!(100),
        };
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(90)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(120)
        );

        let order = LimitOrder::new(Side::Buy, quote!(95), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(3, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(185)
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut account = Account::new(quote!(1000), leverage!(1), fee!(0.0002), fee!(0.0006));
        *account.position_mut() = Position {
            size: base!(-1),
            entry_price: quote!(100),
            margin: quote!(100),
        };
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Buy, quote!(90), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(0, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(0)
        );

        let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(1, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(100)
        );

        let order = LimitOrder::new(Side::Sell, quote!(120), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(2, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(220)
        );

        let order = LimitOrder::new(Side::Buy, quote!(95), base!(1)).unwrap();
        let meta = ExchangeOrderMeta::new(3, 0);
        let order = order.into_pending(meta);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(
                account.position(),
                account.active_limit_orders(),
                account.leverage()
            ),
            quote!(220)
        );
    }
}
