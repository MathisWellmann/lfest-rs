use crate::{
    mock_exchange_linear,
    prelude::*,
};

#[test]
fn cancel_limit_order() {
    let mut exchange = mock_exchange_linear();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();

    let limit_price = QuoteCurrency::new(100, 0);
    let qty = BaseCurrency::one();
    let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();

    exchange.submit_limit_order(order.clone()).unwrap();

    let order_id: OrderId = 0.into();
    let meta = ExchangeOrderMeta::new(order_id, 0.into());
    let expected_order = order.into_pending(meta);

    assert_eq!(exchange.account().active_limit_orders().num_active(), 1);
    assert_eq!(
        exchange
            .account()
            .active_limit_orders()
            .get_by_id(order_id, Side::Buy)
            .unwrap(),
        &expected_order
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(900, 0))
            .position_margin(QuoteCurrency::zero())
            .total_fees_paid(QuoteCurrency::zero())
            .build()
    );
    assert_eq!(
        exchange.account().order_margin(init_margin_req),
        QuoteCurrency::new(100, 0)
    );

    exchange
        .cancel_limit_order(CancelBy::OrderId(order_id))
        .unwrap();
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(1000, 0))
            .position_margin(QuoteCurrency::zero())
            .total_fees_paid(QuoteCurrency::zero())
            .build()
    );
    assert_eq!(
        exchange.account().order_margin(init_margin_req),
        Zero::zero()
    );

    let invalid_id: OrderId = 0.into();
    assert_eq!(
        exchange.cancel_limit_order(CancelBy::OrderId(invalid_id)),
        Err(CancelLimitOrderError::OrderIdNotFound(
            OrderIdNotFound::OrderId(invalid_id)
        ))
    );
}
