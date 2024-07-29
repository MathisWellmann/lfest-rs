use crate::{
    base, bba, mock_exchange_linear,
    prelude::{Currency, ExchangeOrderMeta, LimitOrder, OrderId, Side, UserBalances},
    quote, TEST_FEE_MAKER,
};

#[test]
fn cancel_limit_order() {
    let mut exchange = mock_exchange_linear();
    exchange
        .update_state(0.into(), &bba!(quote!(100), quote!(101)))
        .unwrap();

    let limit_price = quote!(100);
    let qty = base!(1);
    let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();

    exchange.submit_limit_order(order.clone()).unwrap();

    let order_id: OrderId = 0.into();
    let meta = ExchangeOrderMeta::new(order_id, 0.into());
    let expected_order = order.into_pending(meta);

    assert_eq!(exchange.active_limit_orders().len(), 1);
    assert_eq!(
        exchange.active_limit_orders().get(&order_id).unwrap(),
        &expected_order
    );
    let fee = qty.convert(limit_price) * TEST_FEE_MAKER;
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(900) - fee,
            position_margin: quote!(0),
            order_margin: quote!(100) + fee
        }
    );

    exchange.cancel_limit_order(order_id).unwrap();
    assert!(exchange.active_limit_orders().is_empty());
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(1000),
            position_margin: quote!(0),
            order_margin: quote!(000)
        }
    );
}
