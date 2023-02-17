//! Test file for the inverse futures mode of the exchange

use lfest::*;

#[test]
fn inv_long_market_win_full() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let value = exchange.account().margin().available_balance() * 0.8;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Buy, size).unwrap();
    exchange.submit_order(o).unwrap();

    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    let fee_base = size * fee_taker;
    let fee_asset1 = fee_base / exchange.bid();

    assert_eq!(exchange.account().position().size(), size);
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.0 - fee_asset1));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.8));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(4),
        round(0.2 - fee_asset1, 4)
    );

    let _ = exchange.update_state(1, bba!(quote!(2000.0), quote!(2000.0)));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.4));

    let size = 800.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    let o = Order::market(Side::Sell, size).unwrap();
    exchange.submit_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(
        exchange.account().margin().wallet_balance().into_rounded(5),
        round(1.4 - fee_asset1 - fee_asset2, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(5),
        round(1.4 - fee_asset1 - fee_asset2, 5)
    );
}

#[test]
fn inv_long_market_loss_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let o = Order::market(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(800.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(0.99952));
    assert_eq!(exchange.account().margin().available_balance().into_rounded(2), base!(0.2));
    assert_eq!(exchange.account().margin().order_margin(), base!(0.0));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.8));

    let _ = exchange.update_state(2, bba!(quote!(800.0), quote!(800.0)));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(-0.2));

    let o = Order::market(Side::Sell, 800.0).unwrap();
    exchange.submit_order(o).unwrap();

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 800.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(
        exchange.account().margin().wallet_balance().into_rounded(5),
        round(0.8 - fee_combined, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(5),
        round(0.8 - fee_combined, 5)
    );
}

#[test]
fn inv_short_market_win_full() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let o = Order::market(Side::Sell, quote!(800.0)).unwrap();
    exchange.submit_order(o).unwrap();
    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    assert_eq!(exchange.account().position().size(), quote!(-800.0));

    let _ = exchange.update_state(1, bba!(quote!(800.0), quote!(800.0)));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.2));

    let o = Order::market(Side::Buy, quote!(800.0)).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    let _ = exchange.update_state(2, bba!(quote!(800.0), quote!(800.0)));

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 800.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.2 - fee_combined));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(exchange.account().margin().available_balance(), base!(1.2 - fee_combined));
}

#[test]
fn inv_short_market_loss_full() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let value = exchange.account().margin().available_balance() * 0.4;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Sell, size).unwrap();
    exchange.submit_order(o).unwrap();

    let _ = exchange.update_state(1, bba!(1000.0, 1000.0));

    let fee_base1 = size * fee_taker;
    let fee_asset1 = fee_base1 / exchange.bid();

    assert_eq!(exchange.account().position().size(), -size);
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.0 - fee_asset1));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.4));
    assert_eq!(exchange.account().margin().available_balance(), base!(0.6 - fee_asset1));

    let _ = exchange.update_state(2, bba!(2000.0, 2000.0));

    let size = 400.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    assert_eq!(exchange.account().position().unrealized_pnl(), base!(-0.2));

    let o = Order::market(Side::Buy, size).unwrap();
    exchange.submit_order(o).unwrap();

    let _ = exchange.update_state(3, bba!(quote!(2000.0), quote!(2000.0)));

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(
        exchange.account().margin().wallet_balance().into_rounded(5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
}

#[test]
fn inv_long_market_win_partial() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(1000.0, 1000.0));

    let value = exchange.account().margin().available_balance() * 0.8;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Buy, size).unwrap();
    exchange.submit_order(o).unwrap();

    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    let fee_base = size * fee_taker;
    let fee_asset1 = fee_base / exchange.bid();

    assert_eq!(exchange.account().position().size(), size);
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.0 - fee_asset1));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.8));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(4),
        round(0.2 - fee_asset1, 4)
    );

    let _ = exchange.update_state(1, bba!(quote!(2000.0), quote!(2000.0)));

    let size = 400.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.4));

    let o = Order::market(Side::Sell, size).unwrap();
    exchange.submit_order(o).unwrap();

    let _ = exchange.update_state(2, bba!(quote!(2000.0), quote!(2000.0)));

    assert_eq!(exchange.account().position().size(), quote!(400.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.2));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.2 - fee_asset1 - fee_asset2));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.4));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
}

#[test]
fn inv_long_market_loss_partial() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let o = Order::market(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_order(o).unwrap();
    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    assert_eq!(exchange.account().position().size(), quote!(800.0));

    let _ = exchange.update_state(1, bba!(quote!(800.0), quote!(800.0)));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(-0.2));

    let o = Order::market(Side::Sell, quote!(400.0)).unwrap();
    exchange.submit_order(o).unwrap();
    let _ = exchange.update_state(1, bba!(quote!(800.0), quote!(800.0)));

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 400.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), quote!(400.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(-0.1));
    assert_eq!(
        exchange.account().margin().wallet_balance().into_rounded(6),
        round(0.9 - fee_combined, 6)
    );
    assert_eq!(exchange.account().margin().position_margin(), base!(0.4));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(6),
        round(0.5 - fee_combined, 6)
    );
}

#[test]
fn inv_short_market_win_partial() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let o = Order::market(Side::Sell, quote!(800.0)).unwrap();
    exchange.submit_order(o).unwrap();
    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    assert_eq!(exchange.account().position().size(), quote!(-800.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(0.99952));
    assert_eq!(exchange.account().margin().available_balance().into_rounded(5), base!(0.19952));
    assert_eq!(exchange.account().margin().order_margin(), base!(0.0));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.8));

    let _ = exchange.update_state(2, bba!(quote!(800.0), quote!(800.0)));

    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.2));

    let o = Order::market(Side::Buy, quote!(400.0)).unwrap();
    exchange.submit_order(o).unwrap();
    let _ = exchange.update_state(3, bba!(quote!(800.0), quote!(800.0)));

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 400.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), quote!(-400.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.1));
    assert_eq!(
        exchange.account().margin().wallet_balance().into_rounded(6),
        round(1.1 - fee_combined, 6)
    );
    assert_eq!(exchange.account().margin().position_margin(), base!(0.4));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(6),
        round(0.7 - fee_combined, 6)
    );
}

#[test]
fn inv_short_market_loss_partial() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let value = exchange.account().margin().available_balance() * 0.8;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Sell, size).unwrap();
    exchange.submit_order(o).unwrap();

    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    let fee_base1 = size * fee_taker;
    let fee_asset1 = fee_base1 / exchange.bid();

    assert_eq!(exchange.account().position().size(), size.into_negative());
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.0 - fee_asset1));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.8));
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(4),
        round(0.2 - fee_asset1, 4)
    );
    let _ = exchange.update_state(1, bba!(quote!(2000.0), quote!(2000.0)));

    let size = quote!(400.0);
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    assert_eq!(exchange.account().position().unrealized_pnl(), base!(-0.4));

    let o = Order::market(Side::Buy, size).unwrap();
    exchange.submit_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(-400.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(-0.2));
    assert_eq!(
        exchange.account().margin().wallet_balance().into_rounded(5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.4);
    assert_eq!(
        exchange.account().margin().available_balance().into_rounded(2),
        round(0.4 - fee_asset1 - fee_asset2, 2)
    );
}

#[test]
fn inv_test_market_roundtrip() {
    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let fee_taker = config.fee_taker();
    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config);
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let value = exchange.account().margin().available_balance() * 0.9;
    let size = exchange.ask() * value;
    let buy_order = Order::market(Side::Buy, size).unwrap();
    exchange.submit_order(buy_order).unwrap();
    let _ = exchange.update_state(1, bba!(quote!(1000.0), quote!(1000.0)));

    let sell_order = Order::market(Side::Sell, size).unwrap();

    exchange.submit_order(sell_order).unwrap();

    let fee_base = size * fee_taker;
    let fee_asset = fee_base / exchange.ask();

    let _ = exchange.update_state(2, bba!(quote!(1000.0), quote!(1000.0)));

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.0 - 2.0 * fee_asset));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(exchange.account().margin().available_balance(), base!(1.0 - 2.0 * fee_asset));

    let size = quote!(900.0);
    let buy_order = Order::market(Side::Buy, size).unwrap();
    exchange.submit_order(buy_order).unwrap();
    let _ = exchange.update_state(3, bba!(quote!(1000.0), quote!(1000.0)));

    let size = quote!(950.0);
    let sell_order = Order::market(Side::Sell, size).unwrap();

    exchange.submit_order(sell_order).unwrap();

    let _ = exchange.update_state(4, bba!(1000.0, 1000.0));

    assert_eq!(exchange.account().position().size(), quote!(-50.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(exchange.account().position().unrealized_pnl(), base!(0.0));
    assert!(exchange.account().margin().wallet_balance() < base!(1.0));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.05));
    assert!(exchange.account().margin().available_balance(gh) < 1.0.into());
}

#[test]
fn inv_execute_limit() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config::new(
        Fee(0.0002),
        Fee(0.0006),
        base!(1.0),
        1.0,
        FuturesTypes::Inverse,
        String::new(),
        true,
    )
    .unwrap();

    let acc_tracker = NoAccountTracker::default();
    let mut exchange = Exchange::new(acc_tracker, config.clone());
    let _ = exchange.update_state(0, bba!(quote!(1000.0), quote!(1000.0)));

    let o = Order::limit(Side::Buy, quote!(900.0), quote!(450.0)).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);
    assert_eq!(exchange.account().margin().wallet_balance(), base!(1.0));
    assert_eq!(exchange.account().margin().available_balance(), base!(0.4999));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(exchange.account().margin().order_margin(), base!(0.5001)); // this includes the fee too

    let (exec_orders, liq) = exchange.update_state(1, bba!(quote!(750.0), quote!(750.0)));
    assert!(!liq);
    assert_eq!(exec_orders.len(), 1);

    assert_eq!(exchange.bid(), quote!(750.0));
    assert_eq!(exchange.ask(), quote!(750.0));
    assert_eq!(exchange.account().active_limit_orders().len(), 0);
    assert_eq!(exchange.account().position().size(), quote!(450.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(900.0));
    assert_eq!(exchange.account().margin().wallet_balance(), base!(0.9999));
    assert_eq!(exchange.account().margin().available_balance(), base!(0.4999));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.5));
    assert_eq!(exchange.account().margin().order_margin(), base!(0.0));

    let o = Order::limit(Side::Sell, quote!(1000.0), quote!(450.0)).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    let _ = exchange.update_state(1, bba!(quote!(1200.0), quote!(1200.0)));

    assert_eq!(exchange.account().active_limit_orders().len(), 0);
    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().margin().position_margin(), base!(0.0));
    assert_eq!(exchange.account().margin().wallet_balance().into_rounded(5), base!(1.04981));
    assert_eq!(exchange.account().margin().available_balance().into_rounded(5), base!(1.04981));

    let o = Order::limit(Side::Sell, quote!(1200.0), quote!(600.0)).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    let _ = exchange.update_state(2, bba!(quote!(1200.1), quote!(1200.1)));
    assert_eq!(exchange.account().position().size(), quote!(-600.0));
    assert_eq!(exchange.account().margin().position_margin().into_rounded(1), base!(0.5));
    assert_eq!(exchange.account().margin().wallet_balance().into_rounded(2), base!(1.05));
    assert_eq!(exchange.account().margin().available_balance().into_rounded(2), base!(0.55));
}
