use hashbrown::HashMap;

use crate::{mock_exchange_linear, prelude::*, risk_engine::RiskError};

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_reject() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    let order = MarketOrder::new(Side::Buy, base!(10)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}
#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_no_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    let order = MarketOrder::new(Side::Buy, base!(5)).unwrap();
    exchange.submit_market_order(order).unwrap();

    // make sure its excuted immediately
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(5),
            entry_price: quote!(101),
            margin: quote!(505),
        }
    );
    assert_eq!(
        exchange.account().available_wallet_balance(),
        // - fee
        quote!(495) - quote!(0.303)
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_long_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    // First enter a long position
    let order = MarketOrder::new(Side::Buy, base!(5)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = quote!(0.3);
    assert_eq!(
        exchange.account().available_wallet_balance(),
        quote!(500) - fee0
    );
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(5),
            entry_price: quote!(100),
            margin: quote!(500)
        }
    );
    assert_eq!(
        exchange.account().active_limit_orders(),
        &HashMap::default()
    );
    assert_eq!(exchange.account().order_margin(), quote!(0));

    // Buy again
    let order = MarketOrder::new(Side::Buy, base!(4)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 = quote!(0.24);
    assert_eq!(
        exchange.account().available_wallet_balance(),
        quote!(100) - fee0 - fee1
    );
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(9),
            entry_price: quote!(100),
            margin: quote!(900)
        }
    );
    assert_eq!(
        exchange.account().active_limit_orders(),
        &HashMap::default()
    );
    assert_eq!(exchange.account().order_margin(), quote!(0));

    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(9),
            entry_price: quote!(100),
            margin: quote!(900),
        }
    );
    assert_eq!(
        exchange.account().available_wallet_balance(),
        // - fee - fee - spread loss
        quote!(100) - quote!(0.3) - quote!(0.24)
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_short_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = quote!(0.54);
    assert_eq!(
        exchange.account().available_wallet_balance(),
        quote!(100) - fee0
    );
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(-9),
            entry_price: quote!(100),
            margin: quote!(900)
        }
    );
    assert_eq!(
        exchange.account().active_limit_orders(),
        &HashMap::default()
    );
    assert_eq!(exchange.account().order_margin(), quote!(0));

    // Now close the position with a buy order
    let order = MarketOrder::new(Side::Buy, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(0),
            entry_price: quote!(100),
            margin: quote!(0),
        }
    );
    assert_eq!(
        exchange.account().available_wallet_balance(),
        // - fee - fee - spread loss
        quote!(1000) - quote!(0.54) - quote!(0.5454) - quote!(9)
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_turnaround_short() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();

    // Close the entire position and buy some more
    let order = MarketOrder::new(Side::Buy, base!(18)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(9),
            entry_price: quote!(100),
            margin: quote!(900),
        }
    );
    assert_eq!(
        exchange.account().available_wallet_balance(),
        // - fee - fee - spread loss
        quote!(100) - quote!(0.5346) - quote!(1.08) - quote!(9)
    );
}
