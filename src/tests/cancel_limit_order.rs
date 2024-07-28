use crate::{
    base, bba, mock_exchange_linear,
    prelude::{ExchangeOrderMeta, LimitOrder, OrderId, Side, UserBalances},
    quote,
};

#[test]
fn cancel_limit_order() {
    let mut exchange = mock_exchange_linear();
    exchange
        .update_state(0.into(), &bba!(quote!(100), quote!(101)))
        .unwrap();

    let order = LimitOrder::new(Side::Buy, quote!(100), base!(1)).unwrap();

    exchange.submit_limit_order(order.clone()).unwrap();

    let order_id: OrderId = 0.into();
    let meta = ExchangeOrderMeta::new(order_id, 0.into());
    let expected_order = order.into_pending(meta);

    assert_eq!(exchange.active_limit_orders().len(), 1);
    assert_eq!(
        exchange.active_limit_orders().get(&order_id).unwrap(),
        &expected_order
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(900),
            position_margin: quote!(0),
            order_margin: quote!(100)
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
