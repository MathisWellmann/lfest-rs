use crate::{mock_exchange_linear, position::PositionInner, prelude::*, trade};

#[test]
#[tracing_test::traced_test]
fn submit_limit_sell_order_no_position() {
    let mut exchange = mock_exchange_linear();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(exchange
        .update_state(0, bba!(quote!(99), quote!(100)))
        .unwrap()
        .is_empty());

    let limit_price = quote!(100);
    let order = LimitOrder::new(Side::Sell, limit_price, base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    assert_eq!(exchange.position(), &Position::Neutral,);

    // Now fill the order
    let ts = 0;
    let meta = ExchangeOrderMeta::new(0, ts);
    let mut order = order.into_pending(meta);
    let limit_price = order.limit_price();
    let filled_order = order
        .fill(order.remaining_quantity(), ts)
        .expect("order is fully filled.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(0, trade!(limit_price, base!(9), Side::Buy))
            .unwrap(),
        vec![expected_order_update]
    );
    exchange
        .update_state(0, bba!(quote!(101), quote!(102)))
        .unwrap();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            base!(9),
            quote!(100),
            &mut exchange.transaction_accounting,
            init_margin_req,
        ))
    );
    let fee = quote!(0.18);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(100) - fee,
            position_margin: quote!(900),
            order_margin: quote!(0)
        }
    );

    // close the position again
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let meta = ExchangeOrderMeta::new(1, ts);
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts)
        .expect("order is fully filled.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(0, trade!(quote!(100), base!(9), Side::Sell))
            .unwrap(),
        vec![expected_order_update]
    );
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(1000) - fee - fee,
            position_margin: quote!(0),
            order_margin: quote!(0)
        }
    );
}

// Test there is a maximum quantity of buy orders the account can post.
#[test]
#[tracing_test::traced_test]
fn submit_limit_sell_order_no_position_max() {
    let mut exchange = mock_exchange_linear();
    assert!(exchange
        .update_state(0, bba!(quote!(99), quote!(100)))
        .unwrap()
        .is_empty());

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
#[tracing_test::traced_test]
fn submit_limit_sell_order_below_bid() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
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
#[tracing_test::traced_test]
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
