use crate::{mock_exchange_linear, prelude::*, trade};

#[test]
#[tracing_test::traced_test]
fn submit_limit_buy_order_no_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    let limit_price = quote!(98);
    let order = LimitOrder::new(Side::Buy, limit_price, base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    assert_eq!(
        exchange.account().position(),
        &Position {
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
            .update_state(0, trade!(quote!(98), base!(5), Side::Sell))
            .unwrap(),
        vec![expected_order_update]
    );
    let bid = quote!(96);
    let ask = quote!(99);
    let order_updates = exchange.update_state(0, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(5),
            entry_price: quote!(98),
            margin: quote!(490),
        }
    );
    let fee = quote!(0.098);
    assert_eq!(
        // Bid is deliberately the same as the entry price
        exchange.account().total_value(limit_price, ask),
        quote!(1000) - fee
    );
    assert_eq!(
        exchange.account().available_wallet_balance(),
        quote!(510) - fee
    );

    // close the position again with a limit order.
    let order = LimitOrder::new(Side::Sell, quote!(98), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(96), quote!(97)))
            .unwrap(),
        Vec::new()
    );

    let meta = ExchangeOrderMeta::new(1, 0);
    let mut order = order.into_pending(meta);
    let limit_price = order.limit_price();
    order.fill(limit_price, order.quantity());
    let expected_order_update = LimitOrderUpdate::FullyFilled(order.into_filled(limit_price, 0));
    assert_eq!(
        exchange
            .update_state(0, trade!(quote!(99), base!(5), Side::Buy))
            .unwrap(),
        vec![expected_order_update]
    );
    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(0),
            // TODO: does not really make sense to have an `entry_price` with a `size` of zero.
            entry_price: quote!(98),
            margin: quote!(0),
        }
    );
    assert_eq!(
        exchange.account().total_value(bid, ask),
        quote!(1000) - fee - fee
    );
}

// Test there is a maximum quantity of buy orders the account can post.
#[test]
fn submit_limit_buy_order_no_position_max() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    let order = LimitOrder::new(Side::Buy, quote!(100), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(4)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order.clone()),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    let order = LimitOrder::new(Side::Sell, quote!(101), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Sell, quote!(101), base!(4)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Sell, quote!(101), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order.clone()),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}

#[test]
fn submit_limit_buy_order_with_long() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );
    let order = MarketOrder::new(Side::Buy, base!(9)).unwrap();
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
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    // Another buy limit order should not work
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    // But sell order should work
    let order = LimitOrder::new(Side::Sell, quote!(101), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let meta = ExchangeOrderMeta::new(2, 0);
    let mut order = order.into_pending(meta);
    let limit_price = order.limit_price();
    order.fill(limit_price, order.quantity());
    let expected_order_update = LimitOrderUpdate::FullyFilled(order.into_filled(limit_price, 0));
    assert_eq!(
        exchange
            .update_state(0, trade!(quote!(101), base!(9), Side::Buy))
            .unwrap(),
        vec![expected_order_update]
    );

    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(0),
            entry_price: quote!(100),
            margin: quote!(0),
        }
    );
}

#[test]
fn submit_limit_buy_order_with_short() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        vec![]
    );
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();

    assert_eq!(
        exchange.account().position(),
        &Position {
            size: base!(-9),
            entry_price: quote!(100),
            margin: quote!(900),
        }
    );

    // Another sell limit order should not work
    let order = LimitOrder::new(Side::Sell, quote!(101), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    // But buy order should work
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let meta = ExchangeOrderMeta::new(2, 0);
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
        exchange.account().position(),
        &Position {
            size: base!(0),
            entry_price: quote!(100),
            margin: quote!(0),
        }
    );
}

// test rejection if the limit price >= ask
#[test]
fn submit_limit_buy_order_above_ask() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        vec![]
    );
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(9)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::OrderError(OrderError::LimitPriceAboveAsk))
    );
}

// With a short position open, be able to open a long position of equal size using a limit order
// TODO: this requires a change in the `IsolatedMarginRiskEngine`
#[test]
fn submit_limit_buy_order_turnaround_short() {
    // let mut exchange = mock_exchange_base();
    // assert_eq!(
    //     exchange
    //         .update_state(0, bba!(quote!(100), quote!(101)))
    //         .unwrap(),
    //     vec![]
    // );
    // let order = Order::market(Side::Sell, base!(9)).unwrap();
    // exchange.submit_limit_order(order).unwrap();

    // let order = LimitOrder::new(Side::Buy, quote!(100), base!(18)).unwrap();
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
