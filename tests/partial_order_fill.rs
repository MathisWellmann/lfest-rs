use lfest::{mock_exchange_base, prelude::*, trade};

#[test]
fn partial_limit_order_fill() {
    // let mut exchange = mock_exchange_base();

    // let _ = exchange
    //     .update_state(1, bba!(quote!(100), quote!(101)))
    //     .unwrap();
    // let o = LimitOrder::new(Side::Buy, quote!(100), base!(2)).unwrap();
    // exchange.submit_limit_order(o).unwrap();

    // let exec_orders = exchange
    //     .update_state(1, trade!(quote!(100), base!(1), Side::Sell))
    //     .unwrap();
    // // Half of the limit order should be executed
    // assert_eq!(exec_orders.len(), 1);
    // let mut expected = LimitOrder::new(Side::Buy, quote!(100), base!(1)).unwrap();
    // assert_eq!(exec_orders[0], expected);

    todo!()
}
