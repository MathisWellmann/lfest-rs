use test_case::test_matrix;

use crate::{
    base, bba, mock_exchange_linear,
    prelude::{BaseCurrency, Error, LimitOrder, OrderId, Side, Trade},
    quote,
};

#[tracing_test::traced_test]
#[test_matrix([base!(1), base!(3), base!(5), base!(10)])]
fn amend_limit_order_qty(new_qty: BaseCurrency) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(1.into(), &bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let new_order = LimitOrder::new(Side::Buy, quote!(100), new_qty).unwrap();
    let existing_id: OrderId = 0.into();
    exchange
        .amend_limit_order(existing_id, new_order.clone())
        .unwrap();
    let new_id: OrderId = 1.into();
    let replaced_order = exchange.active_limit_orders().get(&new_id).unwrap();
    assert_eq!(replaced_order.limit_price(), new_order.limit_price());
    assert_eq!(
        replaced_order.remaining_quantity(),
        new_order.remaining_quantity()
    );
    assert_eq!(replaced_order.side(), new_order.side());
}

#[tracing_test::traced_test]
#[test_matrix([base!(1), base!(2), base!(3)])]
fn amend_limit_order_qty_with_partial_fill_leading_to_cancel(new_qty: BaseCurrency) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(1.into(), &bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    exchange
        .update_state(
            0.into(),
            &Trade {
                price: quote!(99),
                quantity: base!(3),
                side: Side::Sell,
            },
        )
        .unwrap();

    let existing_id: OrderId = 0.into();
    assert_eq!(
        exchange
            .active_limit_orders()
            .get(&existing_id)
            .unwrap()
            .remaining_quantity(),
        base!(2)
    );

    let new_order = LimitOrder::new(Side::Buy, quote!(100), new_qty).unwrap();
    assert_eq!(
        exchange.amend_limit_order(existing_id, new_order.clone()),
        Err(Error::AmendQtyAlreadyFilled)
    );
}

#[tracing_test::traced_test]
#[test_matrix([base!(4), base!(5), base!(6)])]
fn amend_limit_order_qty_with_partial_fill(new_qty: BaseCurrency) {
    let mut exchange = mock_exchange_linear();

    assert!(exchange
        .update_state(1.into(), &bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    exchange
        .update_state(
            0.into(),
            &Trade {
                price: quote!(99),
                quantity: base!(3),
                side: Side::Sell,
            },
        )
        .unwrap();

    let existing_id: OrderId = 0.into();
    assert_eq!(
        exchange
            .active_limit_orders()
            .get(&existing_id)
            .unwrap()
            .remaining_quantity(),
        base!(2)
    );

    let new_order = LimitOrder::new(Side::Buy, quote!(100), new_qty).unwrap();
    exchange
        .amend_limit_order(existing_id, new_order.clone())
        .unwrap();
    let new_id: OrderId = 1.into();
    let replaced_order = exchange.active_limit_orders().get(&new_id).unwrap();
    assert_eq!(replaced_order.limit_price(), new_order.limit_price());
    assert_eq!(replaced_order.side(), new_order.side());
    let delta = new_qty - base!(5);
    assert_eq!(replaced_order.remaining_quantity(), base!(2) + delta);
}
