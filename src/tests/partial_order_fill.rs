use test_case::test_case;

use crate::{mock_exchange_base, prelude::*, trade};

#[test_case(quote!(100), base!(2), Side::Buy; "With buy order")]
#[test_case(quote!(101), base!(2), Side::Sell; "With sell order")]
fn partial_limit_order_fill(price: QuoteCurrency, qty: BaseCurrency, side: Side) {
    let mut exchange = mock_exchange_base();

    let _ = exchange
        .update_state(1, bba!(quote!(100), quote!(101)))
        .unwrap();
    let order = LimitOrder::new(side, price, qty).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let exec_orders = exchange
        .update_state(1, trade!(price, qty / base!(2), side.inverted()))
        .unwrap();
    // Half of the limit order should be executed
    assert_eq!(exec_orders.len(), 1);

    let meta = ExchangeOrderMeta::new(0, 1);
    let mut order = order.into_pending(meta);
    let limit_price = order.limit_price();
    order.fill(limit_price, qty / base!(2));
    let expected_order_update = LimitOrderUpdate::PartiallyFilled(order);
    assert_eq!(exec_orders[0], expected_order_update);
}
