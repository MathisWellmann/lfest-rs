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
    let mut buy_notional_value_sum = active_limit_orders
        .values()
        .filter(|order| matches!(order.side(), Side::Buy))
        .map(|order| {
            let notional_value = order
                .quantity()
                .convert(order.limit_price().expect(EXPECT_LIMIT_PRICE));
            let fee = notional_value * fee;
            notional_value + fee
        })
        .fold(M::new_zero(), |acc, x| acc + x);

    if position.size() < M::PairedCurrency::new_zero() {
        // Offset the limit order cost by a potential short position
        buy_notional_value_sum = max(
            buy_notional_value_sum
                - min(position.size(), M::PairedCurrency::new_zero())
                    .abs()
                    .convert(position.entry_price),
            M::new_zero(),
        );
    }

    // The sell orders dominate
    let mut sell_notional_value_sum = active_limit_orders
        .values()
        .filter(|order| matches!(order.side(), Side::Sell))
        .map(|order| {
            let notional_value = order
                .quantity()
                .convert(order.limit_price().expect(EXPECT_LIMIT_PRICE));
            let fee = notional_value * fee;
            notional_value + fee
        })
        .fold(M::new_zero(), |acc, x| acc + x);

    if position.size() > M::PairedCurrency::new_zero() {
        // Offset the limit order cost by a potential long position
        sell_notional_value_sum = max(
            M::new_zero(),
            sell_notional_value_sum
                - max(M::PairedCurrency::new_zero(), position.size()).convert(position.entry_price),
        );
    }

    max(buy_notional_value_sum, sell_notional_value_sum) / position.leverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn account_order_margin_no_position() {
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
    fn account_order_margin_with_long() {
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
            quote!(120) + quote!(0.044)
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
    fn account_order_margin_with_short() {
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
