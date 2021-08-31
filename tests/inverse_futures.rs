//! Test file for the inverse futures mode of the exchange

use lfest::*;

#[test]
fn submit_order_limit() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    // submit working market order
    let o = Order::market(Side::Buy, 500.0).unwrap();
    exchange.submit_order(o).unwrap();

    let o = Order::limit(Side::Buy, 900.0, 250.0).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    // submit opposite limit order acting as target order
    let o = Order::limit(Side::Sell, 1200.0, 500.0).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 2);
}

#[test]
fn test_handle_limit_order() {
    // TODO:
}

#[test]
fn handle_stop_market_order_w_trade() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let o = Order::stop_market(Side::Buy, 1010.0, 100.0).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_stop_orders().len(), 1);

    exchange.update_state(1010.0, 1010.0, 1, 1010.0, 1010.0);

    assert_eq!(exchange.account().position().size(), 100.0);
    assert_eq!(exchange.account().position().entry_price(), 1010.0);
}

#[test]
fn inv_long_market_win_full() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let value = exchange.account().margin().available_balance() * 0.8;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Buy, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    let fee_base = size * fee_taker;
    let fee_asset1 = fee_base / exchange.bid();

    assert_eq!(exchange.account().position().size(), size);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.0 - fee_asset1
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.8);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 4),
        round(0.2 - fee_asset1, 4)
    );

    exchange.update_state(2000.0, 2000.0, 1, 2000.0, 2000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.4);

    let size = 800.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    let o = Order::market(Side::Sell, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.4 - fee_asset1 - fee_asset2
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(
        exchange.account().margin().available_balance(),
        1.4 - fee_asset1 - fee_asset2
    );
}

#[test]
fn inv_long_market_loss_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config {
        fee_maker: 0.0,
        fee_taker: 0.0,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let o = Order::market(Side::Buy, 800.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    assert_eq!(exchange.account().position().size(), 800.0);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 1.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 2),
        0.2
    );
    assert_eq!(exchange.account().margin().order_margin(), 0.0);
    assert_eq!(exchange.account().margin().position_margin(), 0.8);

    exchange.update_state(800.0, 800.0, 2, 800.0, 800.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), -0.2);

    let o = Order::market(Side::Sell, 800.0).unwrap();
    exchange.submit_order(o).unwrap();
    exchange.update_state(800.0, 800.0, 3, 800.0, 800.0);

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 800.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 5),
        round(0.8 - fee_combined, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 5),
        round(0.8 - fee_combined, 5)
    );
}

#[test]
fn inv_short_market_win_full() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let o = Order::market(Side::Sell, 800.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    assert_eq!(exchange.account().position().size(), -800.0);

    exchange.update_state(800.0, 800.0, 1, 800.0, 800.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.2);

    let o = Order::market(Side::Buy, 800.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(800.0, 800.0, 2, 800.0, 800.0);

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 800.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.2 - fee_combined
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(
        exchange.account().margin().available_balance(),
        1.2 - fee_combined
    );
}

#[test]
fn inv_short_market_loss_full() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let value = exchange.account().margin().available_balance() * 0.4;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Sell, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    let fee_base1 = size * fee_taker;
    let fee_asset1 = fee_base1 / exchange.bid();

    assert_eq!(exchange.account().position().size(), -size);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.0 - fee_asset1
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.4);
    assert_eq!(
        exchange.account().margin().available_balance(),
        0.6 - fee_asset1
    );

    exchange.update_state(2000.0, 2000.0, 2, 2000.0, 2000.0);

    let size = 400.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    assert_eq!(exchange.account().position().unrealized_pnl(), -0.2);

    let o = Order::market(Side::Buy, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    exchange.update_state(2000.0, 2000.0, 3, 2000.0, 2000.0);

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
}

#[test]
fn inv_long_market_win_partial() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let value = exchange.account().margin().available_balance() * 0.8;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Buy, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    let fee_base = size * fee_taker;
    let fee_asset1 = fee_base / exchange.bid();

    assert_eq!(exchange.account().position().size(), size);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.0 - fee_asset1
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.8);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 4),
        round(0.2 - fee_asset1, 4)
    );

    exchange.update_state(2000.0, 2000.0, 1, 2000.0, 2000.0);

    let size = 400.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    assert_eq!(exchange.account().position().unrealized_pnl(), 0.4);

    let o = Order::market(Side::Sell, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    exchange.update_state(2000.0, 2000.0, 2, 2000.0, 2000.0);

    assert_eq!(exchange.account().position().size(), 400.0);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.2);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.2 - fee_asset1 - fee_asset2
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.4);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
}

#[test]
fn inv_long_market_loss_partial() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let o = Order::market(Side::Buy, 800.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    assert_eq!(exchange.account().position().size(), 800.0);

    exchange.update_state(800.0, 800.0, 1, 800.0, 800.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), -0.2);

    let o = Order::market(Side::Sell, 400.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(800.0, 800.0, 1, 800.0, 800.0);

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 400.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), 400.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), -0.1);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 6),
        round(0.9 - fee_combined, 6)
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.4);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 6),
        round(0.5 - fee_combined, 6)
    );
}

#[test]
fn inv_short_market_win_partial() {
    let config = Config {
        fee_maker: 0.0,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let o = Order::market(Side::Sell, 800.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    assert_eq!(exchange.account().position().size(), -800.0);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 0.9994);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 3),
        0.199
    );
    assert_eq!(exchange.account().margin().order_margin(), 0.0);
    assert_eq!(exchange.account().margin().position_margin(), 0.8);

    exchange.update_state(800.0, 800.0, 2, 800.0, 800.0);

    assert_eq!(exchange.account().position().unrealized_pnl(), 0.2);

    let o = Order::market(Side::Buy, 400.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    exchange.update_state(800.0, 800.0, 3, 800.0, 800.0);

    let fee_base0 = fee_taker * 800.0;
    let fee_asset0 = fee_base0 / 1000.0;

    let fee_base1 = fee_taker * 400.0;
    let fee_asset1 = fee_base1 / 800.0;

    let fee_combined = fee_asset0 + fee_asset1;

    assert_eq!(exchange.account().position().size(), -400.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.1);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 6),
        round(1.1 - fee_combined, 6)
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.4);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 6),
        round(0.7 - fee_combined, 6)
    );
}

#[test]
fn inv_short_market_loss_partial() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let value = exchange.account().margin().available_balance() * 0.8;
    let size = exchange.ask() * value;
    let o = Order::market(Side::Sell, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    let fee_base1 = size * fee_taker;
    let fee_asset1 = fee_base1 / exchange.bid();

    assert_eq!(exchange.account().position().size(), -size);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.0 - fee_asset1
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.8);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 4),
        round(0.2 - fee_asset1, 4)
    );
    exchange.update_state(2000.0, 2000.0, 1, 2000.0, 2000.0);

    let size = 400.0;
    let fee_base2 = size * fee_taker;
    let fee_asset2 = fee_base2 / 2000.0;

    assert_eq!(exchange.account().position().unrealized_pnl(), -0.4);

    let o = Order::market(Side::Buy, size).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());

    assert_eq!(exchange.account().position().size(), -400.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), -0.2);
    assert_eq!(
        round(exchange.account().margin().wallet_balance(), 5),
        round(0.8 - fee_asset1 - fee_asset2, 5)
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.4);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 2),
        round(0.4 - fee_asset1 - fee_asset2, 2)
    );
}

#[test]
fn inv_test_market_roundtrip() {
    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.00075,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let fee_taker = config.fee_taker;
    let mut exchange = Exchange::new(config);
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let value = exchange.account().margin().available_balance() * 0.9;
    let size = exchange.ask() * value;
    let buy_order = Order::market(Side::Buy, size).unwrap();
    let order_err = exchange.submit_order(buy_order);
    assert!(order_err.is_ok());
    exchange.update_state(1000.0, 1000.0, 1, 1000.0, 1000.0);

    let sell_order = Order::market(Side::Sell, size).unwrap();

    let order_err = exchange.submit_order(sell_order);
    assert!(order_err.is_ok());

    let fee_base = size * fee_taker;
    let fee_asset = fee_base / exchange.ask();

    exchange.update_state(1000.0, 1000.0, 2, 1000.0, 1000.0);

    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert_eq!(
        exchange.account().margin().wallet_balance(),
        1.0 - 2.0 * fee_asset
    );
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(
        exchange.account().margin().available_balance(),
        1.0 - 2.0 * fee_asset
    );

    let size = 900.0;
    let buy_order = Order::market(Side::Buy, size).unwrap();
    let order_err = exchange.submit_order(buy_order);
    assert!(order_err.is_ok());
    exchange.update_state(1000.0, 1000.0, 3, 1000.0, 1000.0);

    let size = 950.0;
    let sell_order = Order::market(Side::Sell, size).unwrap();

    let order_err = exchange.submit_order(sell_order);
    assert!(order_err.is_ok());

    exchange.update_state(1000.0, 1000.0, 4, 1000.0, 1000.0);

    assert_eq!(exchange.account().position().size(), -50.0);
    assert_eq!(exchange.account().position().entry_price(), 1000.0);
    assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
    assert!(exchange.account().margin().wallet_balance() < 1.0);
    assert_eq!(exchange.account().margin().position_margin(), 0.05);
    assert!(exchange.account().margin().available_balance() < 1.0);
}

#[test]
fn check_liquidation() {
    // TODO:
}

#[test]
fn test_liquidate() {
    // TODO:
}

#[test]
fn inv_execute_limit() {
    let config = Config {
        fee_maker: 0.0,
        fee_taker: 0.001,
        starting_balance: 1.0,
        leverage: 1.0,
        futures_type: FuturesType::Inverse,
    };
    let mut exchange = Exchange::new(config.clone());
    exchange.update_state(1000.0, 1000.0, 0, 1000.0, 1000.0);

    let o: Order = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);
    assert_eq!(exchange.account().margin().available_balance(), 0.5);
    assert_eq!(exchange.account().margin().order_margin(), 0.5);

    let (exec_orders, liq) = exchange.update_state(750.0, 750.0, 1, 750.0, 750.0);
    assert!(!liq);
    assert_eq!(exec_orders.len(), 1);

    assert_eq!(exchange.bid(), 750.0);
    assert_eq!(exchange.ask(), 750.0);
    assert_eq!(exchange.account().active_limit_orders().len(), 0);
    assert_eq!(exchange.account().position().size(), 450.0);
    assert_eq!(exchange.account().position().entry_price(), 900.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 1.0);

    let o: Order = Order::limit(Side::Sell, 1000.0, 450.0).unwrap();
    let order_err = exchange.submit_order(o);
    assert!(order_err.is_ok());
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    exchange.update_state(1200.0, 1200.0, 1, 1200.0, 1200.0);

    assert_eq!(exchange.account().active_limit_orders().len(), 0);
    assert_eq!(exchange.account().position().size(), 0.0);
    assert_eq!(exchange.account().margin().position_margin(), 0.0);
    assert_eq!(exchange.account().margin().wallet_balance(), 1.05);
    assert_eq!(exchange.account().margin().available_balance(), 1.05);

    let o: Order = Order::limit(Side::Sell, 1200.0, 600.0).unwrap();
    exchange.submit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    exchange.update_state(1200.0, 1201.0, 2, 1201.0, 1200.0);
    assert_eq!(exchange.account().position().size(), -600.0);
    assert_eq!(round(exchange.account().margin().position_margin(), 1), 0.5);
    assert_eq!(round(exchange.account().margin().wallet_balance(), 2), 1.05);
    assert_eq!(
        round(exchange.account().margin().available_balance(), 2),
        0.55
    );
}
