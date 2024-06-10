use test_case::test_case;

use crate::{mock_exchange_linear, prelude::*, trade};

#[tracing_test::traced_test]
#[test_case(quote!(100), base!(2), Side::Buy; "With buy order")]
#[test_case(quote!(101), base!(2), Side::Sell; "With sell order")]
fn partial_limit_order_fill(price: QuoteCurrency, qty: BaseCurrency, side: Side) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(1.into(), bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(side, price, qty).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let exec_orders = exchange
        .update_state(1.into(), trade!(price, qty / base!(2), side.inverted()))
        .unwrap();
    // Half of the limit order should be executed
    assert_eq!(exec_orders.len(), 1);

    let ts = 1;
    let meta = ExchangeOrderMeta::new(0.into(), ts.into());
    let mut order = order.into_pending(meta);
    assert!(order.fill(qty / base!(2), ts.into()).is_none());
    let expected_order_update = LimitOrderUpdate::PartiallyFilled(order);
    assert_eq!(exec_orders[0], expected_order_update);
}
