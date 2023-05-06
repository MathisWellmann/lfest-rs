use crate::{mock_exchange_base, prelude::*, risk_engine::RiskError};

#[test]
fn submit_market_sell_order_reject() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        vec![]
    );

    let order = Order::market(Side::Sell, base!(10)).unwrap();
    assert_eq!(
        exchange.submit_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}

#[test]
fn submit_market_sell_order() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        vec![]
    );

    let order = Order::market(Side::Sell, base!(5)).unwrap();
    exchange.submit_order(order).unwrap();
    // make sure its excuted immediately
    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(-5),
            entry_price: quote!(100),
            position_margin: quote!(500),
            leverage: leverage!(1),
        }
    );
    assert_eq!(
        exchange.account().available_balance(),
        // - fee
        quote!(500) - quote!(0.3)
    );
}

#[test]
fn submit_market_sell_order_with_short_position() {
    todo!()
}

#[test]
fn submit_market_sell_order_with_long_position() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );

    // First enter a long position
    let order = Order::market(Side::Buy, base!(9)).unwrap();
    exchange.submit_order(order).unwrap();

    // Now close the position with a sell order
    let order = Order::market(Side::Sell, base!(9)).unwrap();
    exchange.submit_order(order).unwrap();
    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(0),
            entry_price: quote!(100),
            position_margin: quote!(0),
            leverage: leverage!(1),
        }
    );
    assert_eq!(
        exchange.account().available_balance(),
        // - fee - fee - spread loss
        quote!(1000) - quote!(0.54) - quote!(0.5346) - quote!(9)
    );
}

#[test]
fn submit_market_order_turnaround_long() {
    todo!()
}
