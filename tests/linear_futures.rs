//! Test file for the linear futures mode of the exchange

use lfest::*;

#[test]
fn lin_long_market_win_full() {
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
    let _ = exchange.update_state(100.0, 100.0, 0, 100.0, 100.0);

    exchange
        .submit_order(Order::market(Side::Buy, 5.0).unwrap())
        .unwrap();
    let _ = exchange.update_state(100.0, 100.0, 0, 100.0, 100.0);

    assert_eq!(exchange.account().position().size(), 5.0);
    assert_eq!(exchange.account().position().entry_price(), 100.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 999.7);
    assert_eq!(exchange.account().margin().position_margin(), 500.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 1),
        499.7
    );

    let _ = exchange.update_state(200.0, 200.0, 1, 200.0, 200.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 500.0);

    exchange
        .submit_order(Order::market(Side::Sell, 5.0).unwrap())
        .unwrap();

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().entry_price(), 100.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 1),
        1499.1
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 1),
        1499.1
    );
}
