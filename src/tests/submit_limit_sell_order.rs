use crate::{
    mock_exchange::MockTransactionAccounting, mock_exchange_linear, prelude::*, test_fee_maker,
    trade,
};

#[test]
#[tracing_test::traced_test]
fn submit_limit_sell_order_no_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(exchange
        .update_state(
            0.into(),
            &bba!(QuoteCurrency::new(99, 0), QuoteCurrency::new(100, 0))
        )
        .unwrap()
        .is_empty());

    let limit_price = QuoteCurrency::new(100, 0);
    let order = LimitOrder::new(Side::Sell, limit_price, BaseCurrency::new(9, 0)).unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    assert_eq!(exchange.position(), &Position::Neutral,);

    // Now fill the order
    let ts = 0;
    let meta = ExchangeOrderMeta::new(0.into(), ts.into());
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts.into())
        .expect("order is fully filled.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(
                0.into(),
                &trade!(
                    QuoteCurrency::new(101, 0),
                    BaseCurrency::new(9, 0),
                    Side::Buy
                )
            )
            .unwrap(),
        vec![expected_order_update]
    );
    exchange
        .update_state(
            0.into(),
            &bba!(QuoteCurrency::new(101, 0), QuoteCurrency::new(102, 0)),
        )
        .unwrap();
    let qty = BaseCurrency::new(9, 0);
    let entry_price = QuoteCurrency::new(100, 0);
    let fee = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_maker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0),
            position_margin: QuoteCurrency::new(900, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
    assert_eq!(
        exchange.position().outstanding_fees(),
        QuoteCurrency::new(18, 2)
    );

    // close the position again
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(9, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let meta = ExchangeOrderMeta::new(1.into(), ts.into());
    let mut order = order.into_pending(meta);
    let filled_order = order
        .fill(order.remaining_quantity(), ts.into())
        .expect("order is fully filled.");
    let expected_order_update = LimitOrderUpdate::FullyFilled(filled_order);
    assert_eq!(
        exchange
            .update_state(
                0.into(),
                &trade!(
                    QuoteCurrency::new(99, 0),
                    BaseCurrency::new(9, 0),
                    Side::Sell
                )
            )
            .unwrap(),
        vec![expected_order_update]
    );
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(1000, 0) - fee - fee,
            position_margin: QuoteCurrency::new(0, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
}

// Test there is a maximum quantity of buy orders the account can post.
#[test]
#[tracing_test::traced_test]
fn submit_limit_sell_order_no_position_max() {
    let mut exchange = mock_exchange_linear();
    assert!(exchange
        .update_state(
            0.into(),
            &bba!(QuoteCurrency::new(99, 0), QuoteCurrency::new(100, 0))
        )
        .unwrap()
        .is_empty());

    let order = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(4, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(1, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();

    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(4, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(2, 0),
    )
    .unwrap();
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
            .update_state(
                0.into(),
                &bba!(QuoteCurrency::new(99, 0), QuoteCurrency::new(100, 0))
            )
            .unwrap(),
        Vec::new()
    );
    let order = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(9, 0),
    )
    .unwrap();
    assert_eq!(
        exchange.submit_limit_order(order),
        Err(Error::OrderError(
            OrderError::GoodTillCrossingRejectedOrder {
                limit_price: QuoteCurrency::new(99, 0),
                away_market_quotation_price: QuoteCurrency::new(99, 0)
            }
        ))
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
    //         .update_state(0, bba!(QuoteCurrency::new(100), QuoteCurrency::new(101)))
    //         .unwrap(),
    //     vec![]
    // );
    // let order = Order::market(Side::Buy, BaseCurrency::new(9)).unwrap();
    // exchange.submit_limit_order(order).unwrap();

    // let order = LimitOrder::new(Side::Sell, QuoteCurrency::new(101), BaseCurrency::new(18)).unwrap();
    // exchange.submit_limit_order(order.clone()).unwrap();

    // // Execute the limit buy order
    // assert_eq!(
    //     exchange
    //         .update_state(0, bba!(QuoteCurrency::new(98), QuoteCurrency::new(99)))
    //         .unwrap(),
    //     vec![order]
    // );
    // assert_eq!(
    //     exchange.account().position(),
    //     &Position {
    //         size: BaseCurrency::new(9),
    //         entry_price: QuoteCurrency::new(100),
    //         position_margin: QuoteCurrency::new(900),
    //         leverage: leverage!(1),
    //     }
    // );
}
