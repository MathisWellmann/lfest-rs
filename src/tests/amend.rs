use test_case::test_matrix;

use crate::{bba, mock_exchange_linear, prelude::*, DECIMALS};

#[tracing_test::traced_test]
#[test_matrix([BaseCurrency::new(1, 0), BaseCurrency::new(3, 0), BaseCurrency::new(5, 0), BaseCurrency::new(10, 0)])]
fn amend_limit_order_qty(new_qty: BaseCurrency<i64, DECIMALS>) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(
            1.into(),
            &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0))
        )
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let new_order = LimitOrder::new(Side::Buy, QuoteCurrency::new(100, 0), new_qty).unwrap();
    let existing_id: OrderId = 0.into();
    exchange
        .amend_limit_order(existing_id, new_order.clone())
        .unwrap();
    let new_id: OrderId = 1.into();
    let replaced_order = exchange.active_limit_orders().get_by_id(new_id).unwrap();
    assert_eq!(replaced_order.limit_price(), new_order.limit_price());
    assert_eq!(
        replaced_order.remaining_quantity(),
        new_order.remaining_quantity()
    );
    assert_eq!(replaced_order.side(), new_order.side());
}

#[tracing_test::traced_test]
#[test_matrix([BaseCurrency::new(1, 0), BaseCurrency::new(2, 0), BaseCurrency::new(3, 0)])]
fn amend_limit_order_qty_with_partial_fill_leading_to_cancel(new_qty: BaseCurrency<i64, DECIMALS>) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(
            1.into(),
            &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0))
        )
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    exchange
        .update_state(
            0.into(),
            &Trade {
                price: QuoteCurrency::new(99, 0),
                quantity: BaseCurrency::new(3, 0),
                side: Side::Sell,
            },
        )
        .unwrap();

    let existing_id: OrderId = 0.into();
    assert_eq!(
        exchange
            .active_limit_orders()
            .get_by_id(existing_id)
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(2, 0)
    );

    let new_order = LimitOrder::new(Side::Buy, QuoteCurrency::new(100, 0), new_qty).unwrap();
    assert_eq!(
        exchange.amend_limit_order(existing_id, new_order.clone()),
        Err(Error::AmendQtyAlreadyFilled)
    );
}

#[tracing_test::traced_test]
#[test_matrix([BaseCurrency::new(4, 0), BaseCurrency::new(5 ,0), BaseCurrency::new(6, 0)])]
fn amend_limit_order_qty_with_partial_fill(new_qty: BaseCurrency<i64, DECIMALS>) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(
            1.into(),
            &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0))
        )
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    exchange
        .update_state(
            0.into(),
            &Trade {
                price: QuoteCurrency::new(99, 0),
                quantity: BaseCurrency::new(3, 0),
                side: Side::Sell,
            },
        )
        .unwrap();

    let existing_id: OrderId = 0.into();
    assert_eq!(
        exchange
            .active_limit_orders()
            .get_by_id(existing_id)
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(2, 0)
    );

    let new_order = LimitOrder::new(Side::Buy, QuoteCurrency::new(100, 0), new_qty).unwrap();
    exchange
        .amend_limit_order(existing_id, new_order.clone())
        .unwrap();
    let new_id: OrderId = 1.into();
    let replaced_order = exchange.active_limit_orders().get_by_id(new_id).unwrap();
    assert_eq!(replaced_order.limit_price(), new_order.limit_price());
    assert_eq!(replaced_order.side(), new_order.side());
    let delta = new_qty - BaseCurrency::new(5, 0);
    assert_eq!(
        replaced_order.remaining_quantity(),
        BaseCurrency::new(2, 0) + delta
    );
}
