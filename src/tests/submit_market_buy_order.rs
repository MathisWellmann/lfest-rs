use crate::{
    mock_exchange::MockTransactionAccounting, mock_exchange_linear, prelude::*, test_fee_taker,
};

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_reject() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(99, 0),
                ask: QuoteCurrency::new(100, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}
#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_no_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    let qty = BaseCurrency::new(5, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();

    // make sure its excuted immediately
    let entry_price = QuoteCurrency::new(101, 0);
    let fee = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
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
            available_wallet_balance: QuoteCurrency::new(495, 0),
            position_margin: QuoteCurrency::new(505, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
    assert_eq!(
        exchange.position().outstanding_fees(),
        QuoteCurrency::new(303, 3)
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_long_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(99, 0),
                ask: QuoteCurrency::new(100, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    // First enter a long position
    let qty = BaseCurrency::new(5, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let entry_price = QuoteCurrency::new(100, 0);
    let fee0 = QuoteCurrency::new(3, 1);
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee0,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &ActiveLimitOrders::new(10));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(500, 0),
            position_margin: QuoteCurrency::new(500, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee0);

    // Buy again
    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(4, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 = QuoteCurrency::new(24, 2);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0),
            position_margin: QuoteCurrency::new(900, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
    let fees = fee0 + fee1;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &ActiveLimitOrders::new(10));
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_short_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(9, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = QuoteCurrency::new(54, 2);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0),
            position_margin: QuoteCurrency::new(900, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fee0,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &ActiveLimitOrders::new(10));

    // Now close the position with a buy order
    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(9, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(1000, 0)
                - QuoteCurrency::new(54, 2)
                - QuoteCurrency::new(5454, 4)
                - QuoteCurrency::new(9, 0),
            position_margin: QuoteCurrency::new(0, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_turnaround_short() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(99, 0),
                ask: QuoteCurrency::new(100, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    // First enter a short position
    let qty = BaseCurrency::new(9, 0);
    let order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let entry_price = QuoteCurrency::new(99, 0);
    let fee_0 = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee_0,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(109, 0),
            position_margin: QuoteCurrency::new(891, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
    assert_eq!(
        exchange.position().outstanding_fees(),
        QuoteCurrency::new(5346, 4)
    );

    // Close the entire position and buy some more
    let qty = BaseCurrency::new(18, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let entry_price = QuoteCurrency::new(100, 0);
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            BaseCurrency::new(9, 0),
            entry_price,
            &mut accounting,
            init_margin_req,
            QuoteCurrency::new(0, 0),
        ))
    );

    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0)
                - QuoteCurrency::new(5346, 4)
                - QuoteCurrency::new(108, 2)
                - QuoteCurrency::new(9, 0),
            position_margin: QuoteCurrency::new(900, 0),
            order_margin: QuoteCurrency::new(0, 0),
            _q: std::marker::PhantomData
        }
    );
}
