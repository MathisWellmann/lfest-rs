use crate::{mock_exchange_base, prelude::*};

#[test]
fn submit_limit_buy_order_no_position() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );

    let order = Order::limit(Side::Buy, quote!(98), base!(5)).unwrap();
    exchange.submit_order(order).unwrap();

    todo!()
}
