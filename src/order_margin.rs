use hashbrown::HashMap;

use crate::{
    exchange::EXPECT_LIMIT_PRICE,
    prelude::Position,
    types::{Currency, MarginCurrency, Order, Side},
    utils::{max, min},
};

/// Compute the current order margin requirement.
pub(crate) fn compute_order_margin<M>(
    position: &Position<M>,
    active_limit_orders: &HashMap<u64, Order<M::PairedCurrency>>,
) -> M
where
    M: Currency + MarginCurrency,
{
    let mut open_buy_quantity: M::PairedCurrency = active_limit_orders
        .values()
        .filter(|order| matches!(order.side(), Side::Buy))
        .map(|order| order.quantity())
        .fold(M::PairedCurrency::new_zero(), |acc, x| acc + x);
    let mut open_sell_quantity: M::PairedCurrency = active_limit_orders
        .values()
        .filter(|order| matches!(order.side(), Side::Sell))
        .map(|order| order.quantity())
        .fold(M::PairedCurrency::new_zero(), |acc, x| acc + x);

    // Offset against the open position size.
    if position.size() > M::PairedCurrency::new_zero() {
        open_sell_quantity = open_sell_quantity - position.size();
    } else {
        open_buy_quantity = open_buy_quantity - position.size().abs();
    }

    let mut buy_notional_value_sum = active_limit_orders
        .values()
        .filter(|order| matches!(order.side(), Side::Buy))
        .map(|order| {
            order
                .quantity()
                .convert(order.limit_price().expect(EXPECT_LIMIT_PRICE))
        })
        .fold(M::new_zero(), |acc, x| acc + x);

    // Offset the limit order cost by a potential short position
    buy_notional_value_sum = max(
        buy_notional_value_sum
            - min(position.size(), M::PairedCurrency::new_zero())
                .abs()
                .convert(position.entry_price),
        M::new_zero(),
    );

    // The sell orders dominate
    let mut sell_notional_value_sum = active_limit_orders
        .values()
        .filter(|order| matches!(order.side(), Side::Sell))
        .map(|order| {
            order
                .quantity()
                .convert(order.limit_price().expect(EXPECT_LIMIT_PRICE))
        })
        .fold(M::new_zero(), |acc, x| acc + x);

    // Offset the limit order cost by a potential long position
    sell_notional_value_sum = max(
        M::new_zero(),
        sell_notional_value_sum
            - max(M::PairedCurrency::new_zero(), position.size()).convert(position.entry_price),
    );

    max(buy_notional_value_sum, sell_notional_value_sum) / position.leverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn account_order_margin_no_position() {
        let mut account = Account::new(quote!(1000), leverage!(1));

        assert_eq!(
            compute_order_margin(account.position(), &account.active_limit_orders),
            quote!(0)
        );

        let mut order = Order::limit(Side::Buy, quote!(90), base!(1)).unwrap();
        order.set_id(0);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(90)
        );

        let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
        order.set_id(1);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(100)
        );

        let mut order = Order::limit(Side::Sell, quote!(120), base!(1)).unwrap();
        order.set_id(2);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(220)
        );
    }

    #[test]
    fn account_order_margin_with_long() {
        let _ = pretty_env_logger::try_init();

        let mut account = Account::new(quote!(1000), leverage!(1));
        account.position = Position {
            size: base!(1),
            entry_price: quote!(100),
            position_margin: quote!(100),
            leverage: leverage!(1),
        };
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(0)
        );

        let mut order = Order::limit(Side::Buy, quote!(90), base!(1)).unwrap();
        order.set_id(0);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(90)
        );

        let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
        order.set_id(1);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(90)
        );

        let mut order = Order::limit(Side::Sell, quote!(120), base!(1)).unwrap();
        order.set_id(2);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(120)
        );

        let mut order = Order::limit(Side::Buy, quote!(95), base!(1)).unwrap();
        order.set_id(3);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(185)
        );
    }

    #[test]
    fn account_order_margin_with_short() {
        let _ = pretty_env_logger::try_init();

        let mut account = Account::new(quote!(1000), leverage!(1));
        account.position = Position {
            size: base!(-1),
            entry_price: quote!(100),
            position_margin: quote!(100),
            leverage: leverage!(1),
        };
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(0)
        );

        let mut order = Order::limit(Side::Buy, quote!(90), base!(1)).unwrap();
        order.set_id(0);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(0)
        );

        let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
        order.set_id(1);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(100)
        );

        let mut order = Order::limit(Side::Sell, quote!(120), base!(1)).unwrap();
        order.set_id(2);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(220)
        );

        let mut order = Order::limit(Side::Buy, quote!(95), base!(1)).unwrap();
        order.set_id(3);
        account.append_limit_order(order);
        assert_eq!(
            compute_order_margin(&account.position, &account.active_limit_orders),
            quote!(220)
        );
    }
}
