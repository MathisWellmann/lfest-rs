use crate::{
    mock_exchange::MockTransactionAccounting, mock_exchange_linear, prelude::*, test_fee_taker,
};

#[test]
fn submit_market_sell_order_reject() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(
                0.into(),
                &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0))
            )
            .unwrap(),
        Vec::new()
    );

    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}

#[test]
fn submit_market_sell_order() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(
                0.into(),
                &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0))
            )
            .unwrap(),
        Vec::new()
    );

    let qty = BaseCurrency::new(5, 0);
    let order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    // make sure its excuted immediately
    let entry_price = QuoteCurrency::new(100, 0);
    let fees = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(500, 0),
            position_margin: QuoteCurrency::new(500, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
    assert_eq!(
        exchange.position().outstanding_fees(),
        QuoteCurrency::new(3, 1)
    );
}

#[test]
fn submit_market_sell_order_with_short_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(
                0.into(),
                &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0))
            )
            .unwrap(),
        Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(500, 0),
            position_margin: QuoteCurrency::new(500, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
    let fee0 = QuoteCurrency::new(3, 1);
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(5, 0),
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fee0,
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::default()
    );

    // Sell again
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 = QuoteCurrency::new(24, 2);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0),
            position_margin: QuoteCurrency::new(900, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fee0 + fee1
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::default()
    );
}

#[test]
fn submit_market_sell_order_with_long_position() {
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

    // First enter a long position
    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(9, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();

    // Now close the position with a sell order
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(9, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            // - fee - fee - spread loss
            available_wallet_balance: QuoteCurrency::new(1000, 0)
                - QuoteCurrency::new(54, 2)
                - QuoteCurrency::new(5346, 4)
                - QuoteCurrency::new(9, 0),
            position_margin: QuoteCurrency::new(0, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
}

#[test]
fn submit_market_sell_order_turnaround_long() {
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

    // First enter a long position
    let qty = BaseCurrency::new(9, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 =
        QuoteCurrency::convert_from(qty, QuoteCurrency::new(100, 0)) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0),
            position_margin: QuoteCurrency::new(900, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fee0,
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::default()
    );

    // Now reverse the position
    let qty = BaseCurrency::new(18, 0);
    let order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 =
        QuoteCurrency::convert_from(qty, QuoteCurrency::new(99, 0)) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0) - fee0 - fee1,
            position_margin: QuoteCurrency::new(891, 0),
            order_margin: QuoteCurrency::new(0, 0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(99, 0),
            &mut accounting,
            init_margin_req,
            QuoteCurrency::new(0, 0),
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::default()
    );
}
