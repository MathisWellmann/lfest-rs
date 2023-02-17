//! Test file for the linear futures mode of the exchange

use lfest::*;

#[test]
fn lin_long_market_win_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        quote!(1000.0),
        1.0,
        FuturesTypes::Linear,
        String::new(),
        true,
    )
    .unwrap();

    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(
        0,
        MarketUpdate::Bba {
            bid: quote!(100.0),
            ask: quote!(100.0),
        },
    );

    exchange.submit_order(Order::market(Side::Buy, base!(5.0)).unwrap()).unwrap();
    let _ = exchange.update_state(
        0,
        MarketUpdate::Bba {
            bid: quote!(100.0),
            ask: quote!(100.0),
        },
    );

    assert_eq!(exchange.account().position().size(), base!(5.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(100.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), quote!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), quote!(999.7));
    assert_eq!(exchange.account().margin().position_margin(), quote!(500.0));
    assert_eq!(exchange.account().margin().available_balance().into_rounded(1), quote!(499.7));

    let _ = exchange.update_state(
        0,
        MarketUpdate::Bba {
            bid: quote!(200.0),
            ask: quote!(200.0),
        },
    );
    assert_eq!(exchange.account().position().unrealized_pnl(), quote!(500.0));

    exchange.submit_order(Order::market(Side::Sell, base!(5.0)).unwrap()).unwrap();

    assert_eq!(exchange.account().position().size(), base!(0.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(100.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), quote!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance().into_rounded(1), quote!(1499.1));
    assert_eq!(exchange.account().margin().position_margin(), quote!(0.0));
    assert_eq!(exchange.account().margin().available_balance().into_rounded(1), quote!(1499.1));
}
