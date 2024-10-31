use test_case::test_case;

use crate::{mock_exchange_linear, prelude::*, DECIMALS};

#[tracing_test::traced_test]
#[test_case(QuoteCurrency::new(100, 0), BaseCurrency::new(2, 0), Side::Buy, QuoteCurrency::new(99, 0); "With buy order")]
#[test_case(QuoteCurrency::new(101, 0), BaseCurrency::new(2, 0), Side::Sell, QuoteCurrency::new(102, 0); "With sell order")]
fn partial_limit_order_fill(
    limit_price: QuoteCurrency<i64, DECIMALS>,
    qty: BaseCurrency<i64, DECIMALS>,
    side: Side,
    trade_price: QuoteCurrency<i64, DECIMALS>,
) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into()
        },)
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(side, limit_price, qty).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let exec_orders = exchange
        .update_state(&Trade {
            price: trade_price,
            quantity: qty / BaseCurrency::new(2, 0),
            side: side.inverted(),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    // Half of the limit order should be executed
    assert_eq!(exec_orders.len(), 1);

    let ts = 1;
    let meta = ExchangeOrderMeta::new(0.into(), ts.into());
    let mut order = order.into_pending(meta);
    assert!(order
        .fill(qty / BaseCurrency::new(2, 0), ts.into())
        .is_none());
    let expected_order_update = LimitOrderUpdate::PartiallyFilled(order);
    assert_eq!(exec_orders[0], expected_order_update);
}
