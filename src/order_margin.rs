use hashbrown::HashMap;

use crate::{
    exchange::EXPECT_LIMIT_PRICE,
    prelude::Position,
    types::{Currency, Fee, MarginCurrency, Order, Side},
    utils::{max, min},
};

/// Compute the current order margin requirement.
pub(crate) fn compute_order_margin<M>(
    position: &Position<M>,
    active_limit_orders: &HashMap<u64, Order<M::PairedCurrency>>,
    fee: Fee,
) -> M
where
    M: Currency + MarginCurrency,
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
    buys.sort_by_key(|order| order.limit_price().expect(EXPECT_LIMIT_PRICE));
    debug!("buys: {:?}", buys);

    let mut sells = Vec::from_iter(
        active_limit_orders
            .values()
            .filter(|order| matches!(order.side(), Side::Sell)),
    );
    sells.sort_by_key(|order| order.limit_price().expect(EXPECT_LIMIT_PRICE));
    debug!("sells: {:?}", sells);

    // accumulate notional value (+fee) of buy orders which are not offset by the position
    let mut buy_margin_req = M::new_zero();
    let mut remaining_short_size = min(position.size(), M::PairedCurrency::new_zero()).abs();
    for b in &buys {
        let mut order_qty = b.quantity();
        if remaining_short_size > M::PairedCurrency::new_zero() {
            // offset the order qty by as much as possible
            let offset = max(order_qty, remaining_short_size);
            order_qty -= offset;
            remaining_short_size -= offset;
        }
        let order_value = order_qty.convert(b.limit_price().expect(EXPECT_LIMIT_PRICE));
        let margin_req = order_value / position.leverage;
        let fee = order_value * fee;
        buy_margin_req = buy_margin_req + margin_req + fee;
    }

    let mut sell_margin_req = M::new_zero();
    let mut remaining_long_size = max(position.size(), M::PairedCurrency::new_zero());
    for s in &sells {
        let mut order_qty = s.quantity();
        if remaining_long_size > M::PairedCurrency::new_zero() {
            // offset the order qty by as much as possible
            let offset = max(order_qty, remaining_long_size);
            order_qty -= offset;
            remaining_long_size -= offset;
        }
        let order_value = order_qty.convert(s.limit_price().expect(EXPECT_LIMIT_PRICE));
        let margin_req = order_value / position.leverage;
        let fee = order_value * fee;
        sell_margin_req = sell_margin_req + margin_req + fee;
    }

    max(buy_margin_req, sell_margin_req)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn order_margin_no_position() {
        let fee = fee!(0.0002);
        let mut account = Account::new(quote!(1000), leverage!(1), fee);

        assert_eq!(
            compute_order_margin(account.position(), &account.active_limit_orders, fee),
            quote!(0)
        );

        let mut order = Order::limit(Side::Buy, quote!(90), base!(1)).unwrap();
        order.set_id(0);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(90) + quote!(0.018)
        );

        let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
        order.set_id(1);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(100) + quote!(0.02)
        );

        let mut order = Order::limit(Side::Sell, quote!(120), base!(1)).unwrap();
        order.set_id(2);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(220) + quote!(0.044)
        );
    }

    #[test]
    fn order_margin_with_long() {
        let _ = pretty_env_logger::try_init();

        let fee = fee!(0.0002);
        let mut account = Account::new(quote!(1000), leverage!(1), fee);
        account.position = Position {
            size: base!(1),
            entry_price: quote!(100),
            position_margin: quote!(100),
            leverage: leverage!(1),
        };
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(0)
        );

        let mut order = Order::limit(Side::Buy, quote!(90), base!(1)).unwrap();
        order.set_id(0);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(90) + quote!(0.018)
        );

        let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
        order.set_id(1);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(90) + quote!(0.018)
        );

        let mut order = Order::limit(Side::Sell, quote!(120), base!(1)).unwrap();
        order.set_id(2);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(120) + quote!(0.024)
        );

        let mut order = Order::limit(Side::Buy, quote!(95), base!(1)).unwrap();
        order.set_id(3);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(185) + quote!(0.037)
        );
    }

    #[test]
    fn order_margin_with_short() {
        let _ = pretty_env_logger::try_init();

        let fee = fee!(0.0002);
        let mut account = Account::new(quote!(1000), leverage!(1), fee);
        account.position = Position {
            size: base!(-1),
            entry_price: quote!(100),
            position_margin: quote!(100),
            leverage: leverage!(1),
        };
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(0)
        );

        let mut order = Order::limit(Side::Buy, quote!(90), base!(1)).unwrap();
        order.set_id(0);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(0)
        );

        let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
        order.set_id(1);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(100) + quote!(0.02)
        );

        let mut order = Order::limit(Side::Sell, quote!(120), base!(1)).unwrap();
        order.set_id(2);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(220) + quote!(0.044)
        );

        let mut order = Order::limit(Side::Buy, quote!(95), base!(1)).unwrap();
        order.set_id(3);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders, fee),
            quote!(220) + quote!(0.044)
        );
    }
}
