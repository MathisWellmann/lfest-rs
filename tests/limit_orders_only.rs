//! Test if a pure limit order strategy works correctly

use lfest::{mock_exchange_base, prelude::*};

#[test]
#[ignore] // TODO: investigate the fee mechanism wholistically
fn limit_orders_only() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_base();
    todo!();

    // let (exec_orders, liq) = exchange
    //     .update_state(
    //         0,
    //         MarketUpdate::Bba {
    //             bid: quote!(100.0),
    //             ask: quote!(100.1),
    //         },
    //     )
    //     .unwrap();
    // assert!(!liq);
    // assert_eq!(exec_orders.len(), 0);

    // let o = Order::limit(Side::Buy, quote!(100.0), base!(9.9)).unwrap();
    // exchange.submit_order(o).unwrap();
    // assert_eq!(exchange.account().margin().order_margin(), quote!(990.198));
    // assert_eq!(
    //     exchange.account().margin().available_balance(),
    //     quote!(9.802)
    // );

    // let (exec_orders, liq) = exchange
    //     .update_state(
    //         1,
    //         MarketUpdate::Bba {
    //             bid: quote!(99.9),
    //             ask: quote!(100.0),
    //         },
    //     )
    //     .unwrap();
    // assert!(!liq);
    // assert_eq!(exec_orders.len(), 1);
    // debug!("exec_orders: {:?}", exec_orders);

    // assert_eq!(exchange.account().position().size(), base!(9.9));
    // assert_eq!(exchange.account().position().entry_price(), quote!(100.0));
    // // TODO: upnl uses mid price but should use the expected fill price, meaning it
    // // should be 0.99 not 0.495.

    // // assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);

    // assert_eq!(
    //     exchange.account().margin().wallet_balance(),
    //     quote!(999.802)
    // );
    // assert_eq!(exchange.account().margin().position_margin(), quote!(990.0));
    // assert_eq!(exchange.account().margin().order_margin(), quote!(0.0));
    // assert_eq!(
    //     exchange.account().margin().available_balance(),
    //     quote!(9.802)
    // );

    // let o = Order::limit(Side::Sell, quote!(105.1), base!(9.9)).unwrap();
    // exchange.submit_order(o).unwrap();
    // assert_eq!(exchange.account().margin().order_margin(), quote!(0.0));

    // let (exec_orders, liq) = exchange
    //     .update_state(
    //         2,
    //         MarketUpdate::Bba {
    //             bid: quote!(106.0),
    //             ask: quote!(106.1),
    //         },
    //     )
    //     .unwrap();
    // assert!(!liq);
    // assert!(!exec_orders.is_empty());

    // assert_eq!(exchange.account().position().size(), base!(0.0));
    // assert_eq!(
    //     exchange.account().margin().wallet_balance(),
    //     quote!(1050.083902)
    // );
    // assert_eq!(exchange.account().margin().position_margin(), quote!(0.0));
    // assert_eq!(exchange.account().margin().order_margin(), quote!(0.0));
    // assert_eq!(
    //     exchange.account().margin().available_balance(),
    //     quote!(1050.083902)
    // );
}

#[test]
fn limit_orders_2() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_base();

    let exec_orders = exchange
        .update_state(
            0,
            MarketUpdate::Bba {
                bid: quote!(100.0),
                ask: quote!(101),
            },
        )
        .unwrap();
    assert!(exec_orders.is_empty());

    let o = Order::limit(Side::Sell, quote!(101), base!(0.75)).unwrap();
    exchange.submit_order(o).unwrap();

    let o = Order::limit(Side::Buy, quote!(100), base!(0.5)).unwrap();
    exchange.submit_order(o).unwrap();

    let exec_orders = exchange
        .update_state(
            1,
            MarketUpdate::Bba {
                bid: quote!(99),
                ask: quote!(100),
            },
        )
        .unwrap();
    assert_eq!(exec_orders.len(), 1);
}
