//! Test if a pure limit order strategy works correctly

use lfest::{mock_exchange_linear, prelude::*, trade};

#[test]
#[tracing_test::traced_test]
fn limit_orders_only() {
    let mut exchange = mock_exchange_linear();

    let bid = quote!(100);
    let ask = quote!(101);
    let exec_orders = exchange.update_state(0, bba!(bid, ask)).unwrap();
    assert_eq!(exec_orders.len(), 0);

    let o = LimitOrder::new(Side::Buy, quote!(100), base!(9.9)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.account().order_margin(), quote!(990));
    assert_eq!(exchange.account().available_wallet_balance(), quote!(10));

    let order_updates = exchange
        .update_state(1, trade!(quote!(100), base!(10), Side::Sell))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let order_updates = exchange
        .update_state(1, bba!(quote!(98), quote!(99)))
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.account().position().size(), base!(9.9));
    assert_eq!(exchange.account().position().entry_price(), quote!(100));

    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        quote!(-19.8)
    );

    assert_eq!(exchange.account().position().margin(), quote!(990));
    assert_eq!(exchange.account().total_value(bid, ask), quote!(999.802));
    assert_eq!(exchange.account().order_margin(), quote!(0.0));
    assert_eq!(exchange.account().available_wallet_balance(), quote!(9.802));

    let o = LimitOrder::new(Side::Sell, quote!(105), base!(9.9)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.account().order_margin(), quote!(0));

    let order_updates = exchange
        .update_state(2, trade!(quote!(105), base!(10), Side::Buy))
        .unwrap();
    assert!(!order_updates.is_empty());
    let order_updates = exchange
        .update_state(2, bba!(quote!(106), quote!(107)))
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.account().position().size(), base!(0.0));
    assert_eq!(
        exchange.account().total_value(bid, ask),
        quote!(1049.5) - quote!(0.198) - quote!(0.2079)
    );
    assert_eq!(exchange.account().position().margin(), quote!(0.0));
    assert_eq!(exchange.account().order_margin(), quote!(0.0));
    assert_eq!(
        exchange.account().available_wallet_balance(),
        quote!(1049.5) - quote!(0.198) - quote!(0.2079)
    );
}

#[test]
#[tracing_test::traced_test]
fn limit_orders_2() {
    let mut exchange = mock_exchange_linear();

    let exec_orders = exchange
        .update_state(
            0,
            Bba {
                bid: quote!(100),
                ask: quote!(101),
            },
        )
        .unwrap();
    assert!(exec_orders.is_empty());

    let o = LimitOrder::new(Side::Sell, quote!(101), base!(0.75)).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let o = LimitOrder::new(Side::Buy, quote!(100), base!(0.5)).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let exec_orders = exchange
        .update_state(1, trade!(quote!(98), base!(2), Side::Sell))
        .unwrap();
    let _ = exchange
        .update_state(1, bba!(quote!(98), quote!(99)))
        .unwrap();
    assert_eq!(exec_orders.len(), 1);
}
