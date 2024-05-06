use crate::{mock_exchange_base, prelude::*, trade};

#[test]
fn submit_limit_sell_order_no_position() {
    let mut exchange = mock_exchange_base();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );

    let order = LimitOrder::new(Side::Sell, quote!(100), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(0),
            entry_price: quote!(0),
            margin: quote!(0),
        }
    );

    // Now fill the order
    let meta = ExchangeOrderMeta::new(0, 0);
    let mut order = order.into_pending(meta);
    let limit_price = order.limit_price();
    order.fill(limit_price, order.quantity());
    let expected_order_update = LimitOrderUpdate::FullyFilled(order.into_filled(limit_price, 0));
    assert_eq!(
        exchange
            .update_state(0, trade!(quote!(100), base!(9), Side::Buy))
            .unwrap(),
        vec![expected_order_update]
    );
    exchange
        .update_state(0, bba!(quote!(101), quote!(102)))
        .unwrap();
    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(-9),
            entry_price: quote!(100),
            margin: quote!(900),
        }
    );
    let fee = quote!(0.18);
    assert_eq!(exchange.account().wallet_balance, quote!(1000) - fee);
    assert_eq!(exchange.account().available_balance(), quote!(100) - fee);

    // close the position again
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let meta = ExchangeOrderMeta::new(1, 0);
    let mut order = order.into_pending(meta);
    let limit_price = order.limit_price();
    order.fill(limit_price, order.quantity());
    let expected_order_update = LimitOrderUpdate::FullyFilled(order.into_filled(limit_price, 0));
    assert_eq!(
        exchange
            .update_state(0, trade!(quote!(100), base!(9), Side::Sell))
            .unwrap(),
        vec![expected_order_update]
    );
    assert_eq!(
        exchange.account().position,
        Position {
            size: base!(0),
            entry_price: quote!(100),
            margin: quote!(0),
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

    let order = LimitOrder::new(Side::Sell, quote!(100), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Sell, quote!(100), base!(4)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Sell, quote!(100), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order.clone()),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    let order = LimitOrder::new(Side::Buy, quote!(99), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Buy, quote!(99), base!(4)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Buy, quote!(99), base!(2)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order.clone()),
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
    let order = LimitOrder::new(Side::Sell, quote!(99), base!(9)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::OrderError(OrderError::LimitPriceBelowBid))
    );
}

// With a long position open, be able to open a short position of equal size using a limit order
// TODO: this requires a change in the `IsolatedMarginRiskEngine`
#[test]
fn submit_limit_sell_order_turnaround_long() {
    // let mut exchange = mock_exchange_base();
    // assert_eq!(
    //     exchange
    //         .update_state(0, bba!(quote!(100), quote!(101)))
    //         .unwrap(),
    //     vec![]
    // );
    // let order = Order::market(Side::Buy, base!(9)).unwrap();
    // exchange.submit_limit_order(order).unwrap();

    // let order = LimitOrder::new(Side::Sell, quote!(101), base!(18)).unwrap();
    // exchange.submit_limit_order(order.clone()).unwrap();

    // // Execute the limit buy order
    // assert_eq!(
    //     exchange
    //         .update_state(0, bba!(quote!(98), quote!(99)))
    //         .unwrap(),
    //     vec![order]
    // );
    // assert_eq!(
    //     exchange.account().position(),
    //     &Position {
    //         size: base!(9),
    //         entry_price: quote!(100),
    //         position_margin: quote!(900),
    //         leverage: leverage!(1),
    //     }
    // );
}
