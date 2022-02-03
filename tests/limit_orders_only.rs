//! Test if a pure limit order strategy works correctly

use lfest::*;
use log::*;

#[test]
fn limit_orders_only() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config::new(
        0.0002,
        0.0006,
        1000.0,
        1.0,
        FuturesTypes::Linear,
        String::new(),
        true,
    )
    .unwrap();

    let mut exchange = Exchange::new(config);

    let (exec_orders, liq) = exchange.update_state(100.0, 100.1, 0, 100.1, 100.0);
    assert!(!liq);
    assert_eq!(exec_orders.len(), 0);

    let o = Order::limit(Side::Buy, 100.0, 9.9).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().margin().order_margin(), 990.198);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 3),
        9.802
    );

    let (exec_orders, liq) = exchange.update_state(99.9, 100.0, 1, 100.0, 99.9);
    assert!(!liq);
    assert_eq!(exec_orders.len(), 1);
    debug!("exec_orders: {:?}", exec_orders);

    assert_eq!(exchange.account().position().size(), 9.9);
    assert_eq!(exchange.account().position().entry_price(), 100.0);
    // TODO: upnl uses mid price but should use the expected fill price, meaning it should be 0.99 not 0.495
    //assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);

    assert_eq!(exchange.account().margin().wallet_balance(), 999.802);
    assert_eq!(exchange.account().margin().position_margin(), 990.0);
    assert_eq!(exchange.account().margin().order_margin(), 0.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 3),
        9.802
    );

    let o = Order::limit(Side::Sell, 105.1, 9.9).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().margin().order_margin(), 0.0);

    let (exec_orders, liq) = exchange.update_state(106.0, 106.1, 2, 106.1, 106.0);
    assert!(!liq);
    assert!(!exec_orders.is_empty());

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 6),
        1050.083902
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(exchange.account().margin().order_margin(), 0.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 6),
        1050.083902
    );
}

#[test]
fn limit_orders_2() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config::new(
        0.0002,
        0.0006,
        100.0,
        1.0,
        FuturesTypes::Linear,
        String::new(),
        true,
    )
    .unwrap();

    let mut exchange = Exchange::new(config);

    let (exec_orders, liq) = exchange.update_state(100.0, 100.1, 0, 100.1, 100.0);
    assert!(!liq);
    assert!(exec_orders.is_empty());

    let o = Order::limit(Side::Sell, 100.1, 0.75).unwrap();
    exchange.submit_order(o).unwrap();

    let o = Order::limit(Side::Buy, 100.0, 0.5).unwrap();
    exchange.submit_order(o).unwrap();

    let (exec_orders, liq) = exchange.update_state(99.0, 99.1, 0, 99.1, 99.0);
    assert!(!liq);
    assert_eq!(exec_orders.len(), 1);
}
