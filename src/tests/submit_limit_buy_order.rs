use crate::{
    mock_exchange::MockTransactionAccounting, mock_exchange_linear, prelude::*, trade,
    TEST_FEE_MAKER, TEST_FEE_TAKER,
};

#[test]
#[tracing_test::traced_test]
fn submit_limit_buy_order_no_position() {
    let mut exchange = mock_exchange_linear();
    assert!(exchange
        .update_state(0.into(), &bba!(quote!(99), quote!(100)))
        .unwrap()
        .is_empty());

    let limit_price = quote!(98);
    let qty = base!(5);
    let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.position(), &Position::Neutral);
    let fee = qty.convert(limit_price) * TEST_FEE_MAKER;
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(510),
            position_margin: quote!(0),
            order_margin: quote!(490)
        }
    );

    // Now fill the order
    let ts = 0;
    let meta = ExchangeOrderMeta::new(0.into(), ts.into());
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts.into())
        .expect("Order is fully filled.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(0.into(), &trade!(quote!(97), base!(5), Side::Sell))
            .unwrap(),
        vec![expected_order_update]
    );
    let bid = quote!(96);
    let ask = quote!(99);
    assert!(exchange
        .update_state(0.into(), &bba!(bid, ask))
        .unwrap()
        .is_empty());
    let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
    let init_margin_req = Dec!(1);
    assert_eq!(
        exchange.position(),
        &Position::Long(PositionInner::new(
            qty,
            limit_price,
            &mut accounting,
            init_margin_req,
            fee,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(510),
            position_margin: quote!(490),
            order_margin: quote!(0),
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), quote!(0.098));

    // close the position again with a limit order.
    let order = LimitOrder::new(Side::Sell, quote!(98), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    assert!(exchange
        .update_state(0.into(), &bba!(quote!(96), quote!(97)))
        .unwrap()
        .is_empty());

    let meta = ExchangeOrderMeta::new(1.into(), 0.into());
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts.into())
        .expect("order is filled with this.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(0.into(), &trade!(quote!(99), base!(5), Side::Buy))
            .unwrap(),
        vec![expected_order_update]
    );
    assert_eq!(exchange.position(), &Position::Neutral);
}

// Test there is a maximum quantity of buy orders the account can post.
#[test]
#[tracing_test::traced_test]
fn submit_limit_buy_order_no_position_max() {
    let mut exchange = mock_exchange_linear();
    assert!(exchange
        .update_state(0.into(), &bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());

    let order = LimitOrder::new(Side::Buy, quote!(100), base!(5)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(4)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(1)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

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
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = quote!(99);
    let ask = quote!(100);
    assert!(exchange
        .update_state(0.into(), &bba!(bid, ask))
        .unwrap()
        .is_empty());
    let qty = base!(9);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();

    let fee = qty.convert(ask) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            base!(9),
            quote!(100),
            &mut accounting,
            init_margin_req,
            fee,
        )),
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(100),
            position_margin: quote!(900),
            order_margin: quote!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), quote!(0.54));

    assert_eq!(
        exchange
            .update_state(0.into(), &bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    // Another buy limit order should not work
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(1.1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    // But sell order should work
    let order = LimitOrder::new(Side::Sell, quote!(101), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let ts = 0;
    let meta = ExchangeOrderMeta::new(2.into(), ts.into());
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts.into())
        .expect("order is fully filled");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(0.into(), &trade!(quote!(102), base!(9), Side::Buy))
            .unwrap(),
        vec![expected_order_update]
    );

    assert_eq!(exchange.position(), &Position::Neutral);
}

#[test]
fn submit_limit_buy_order_with_short() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(exchange
        .update_state(0.into(), &bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();

    let qty = base!(9);
    let entry_price = quote!(100);
    let fee = qty.convert(entry_price) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(100),
            position_margin: quote!(900),
            order_margin: quote!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), quote!(0.54));

    // Another sell limit order should not work
    let order = LimitOrder::new(Side::Sell, quote!(101), base!(1)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );

    // But buy order should work
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(9)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let ts = 0;
    let meta = ExchangeOrderMeta::new(2.into(), ts.into());
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts.into())
        .expect("Order is filled with this.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(0.into(), &trade!(quote!(99), base!(9), Side::Sell))
            .unwrap(),
        vec![expected_order_update]
    );

    assert_eq!(exchange.position(), &Position::Neutral);
}

// test rejection if the limit price >= ask
#[test]
fn submit_limit_buy_order_above_ask() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0.into(), &bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );
    let order = LimitOrder::new(Side::Buy, quote!(100), base!(9)).unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::OrderError(
            OrderError::GoodTillCrossingRejectedOrder {
                limit_price: quote!(100),
                away_market_quotation_price: quote!(100)
            }
        ))
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
