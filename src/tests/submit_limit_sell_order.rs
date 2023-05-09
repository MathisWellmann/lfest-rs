use crate::{mock_exchange_base, prelude::*};

fn submit_limit_sell_order_no_position() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );

    let mut order = Order::limit(Side::Sell, quote!(100), base!(9)).unwrap();
    exchange.submit_order(order.clone()).unwrap();

    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(0),
            entry_price: quote!(0),
            position_margin: quote!(0),
            leverage: leverage!(1),
        }
    );

    // Now fill the order
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(101), quote!(102)))
            .unwrap(),
        vec![order]
    );
    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(9),
            entry_price: quote!(100),
            position_margin: quote!(900),
            leverage: leverage!(1),
        }
    );
    let fee = quote!(0.1);
    assert_eq!(exchange.account().wallet_balance, quote!(1000) - fee);
    assert_eq!(exchange.account().available_balance(), quote!(900) - fee);

    // close the position again
    let mut order = Order::limit(Side::Buy, quote!(100), base!(9)).unwrap();
    exchange.submit_order(order.clone()).unwrap();

    order.set_id(1);
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![order]
    );
    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(0),
            entry_price: quote!(100),
            position_margin: quote!(0),
            leverage: leverage!(1),
        }
    );
    assert_eq!(exchange.account().wallet_balance, quote!(1000) - fee - fee);
    assert_eq!(
        exchange.account().available_balance(),
        quote!(1000) - fee - fee
    );
}

// Test there is a maximum quantity of buy orders the account can post.
#[test]
fn submit_limit_sell_order_no_position_max() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );

    let mut order = Order::limit(Side::Sell, quote!(100), base!(5)).unwrap();
    exchange.submit_order(order.clone()).unwrap();
    let mut order = Order::limit(Side::Sell, quote!(100), base!(4)).unwrap();
    exchange.submit_order(order.clone()).unwrap();
    let mut order = Order::limit(Side::Sell, quote!(100), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_order(order.clone()),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    let mut order = Order::limit(Side::Buy, quote!(99), base!(5)).unwrap();
    exchange.submit_order(order.clone()).unwrap();
    let mut order = Order::limit(Side::Buy, quote!(99), base!(4)).unwrap();
    exchange.submit_order(order.clone()).unwrap();
    let mut order = Order::limit(Side::Buy, quote!(99), base!(2)).unwrap();
    assert_eq!(
        exchange.submit_order(order.clone()),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}

#[test]
fn submit_limit_sell_order_below_bid() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );
    let order = Order::limit(Side::Sell, quote!(99), base!(9)).unwrap();
    assert_eq!(
        exchange.submit_order(order),
        Err(Error::OrderError(OrderError::LimitPriceBelowBid))
    );
}

#[test]
fn submit_limit_sell_order_turnaround_long() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        vec![]
    );
    let order = Order::market(Side::Buy, base!(9)).unwrap();
    exchange.submit_order(order).unwrap();

    let order = Order::limit(Side::Sell, quote!(101), base!(18)).unwrap();
    exchange.submit_order(order.clone()).unwrap();

    // Execute the limit buy order
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(98), quote!(99)))
            .unwrap(),
        vec![order]
    );
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(9),
            entry_price: quote!(100),
            position_margin: quote!(900),
            leverage: leverage!(1),
        }
    );
}
