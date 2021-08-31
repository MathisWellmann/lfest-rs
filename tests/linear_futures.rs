//! Test file for the linear futures mode of the exchange

use lfest::*;

#[test]
fn lin_long_market_win_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config {
        fee_maker: 0.0,
        fee_taker: 0.0,
        starting_balance: 1000.0,
        leverage: 1.0,
        futures_type: FuturesType::Linear,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(100.0, 100.0, 0, 100.0, 100.0);

    assert!(exchange
        .submit_order(Order::market(Side::Buy, 9.0).unwrap())
        .is_ok());
    exchange.update_state(100.0, 100.0, 0, 100.0, 100.0);

    assert_eq!(exchange.account().position().size(), 9.0);
    assert_eq!(exchange.account().position().entry_price(), 100.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 1000.0);
    assert_eq!(exchange.account().margin().position_margin(), 900.0);
    assert_eq!(exchange.account().margin().available_balance(), 100.0);

    exchange.update_state(200.0, 200.0, 1, 200.0, 200.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 900.0);

    assert!(exchange
        .submit_order(Order::market(Side::Sell, 9.0).unwrap())
        .is_ok());

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().entry_price(), 100.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 1900.0);
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(exchange.account().margin().available_balance(), 1900.0);
}