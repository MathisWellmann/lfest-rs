//! Test if a pure limit order strategy works correctly

use lfest::{
    mock_exchange_linear, mock_exchange_linear_with_account_tracker, prelude::*, trade,
    MockTransactionAccounting,
};
use num_traits::Zero;

#[test]
#[tracing_test::traced_test]
fn limit_orders_only() {
    let mut exchange = mock_exchange_linear_with_account_tracker(QuoteCurrency::new(1000, 0));

    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let fee_maker = exchange.config().contract_spec().fee_maker();

    let bid = QuoteCurrency::new(100, 0);
    let ask = QuoteCurrency::new(101, 0);
    let exec_orders = exchange.update_state(0.into(), &bba!(bid, ask)).unwrap();
    assert_eq!(exec_orders.len(), 0);

    let qty = BaseCurrency::new(99, 1);
    let fee0 = QuoteCurrency::convert_from(qty, bid) * *fee_maker.as_ref();
    let o = LimitOrder::new(Side::Buy, bid, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(10, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::new(990, 0),
        }
    );
    assert_eq!(
        exchange.position().outstanding_fees(),
        QuoteCurrency::zero()
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 1);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(
        exchange.account_tracker().buy_volume(),
        QuoteCurrency::zero()
    );
    assert_eq!(
        exchange.account_tracker().sell_volume(),
        QuoteCurrency::zero()
    );
    assert_eq!(exchange.fees_paid(), QuoteCurrency::zero());

    let order_updates = exchange
        .update_state(
            1.into(),
            &trade!(
                QuoteCurrency::new(99, 0),
                BaseCurrency::new(10, 0),
                Side::Sell
            ),
        )
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let order_updates = exchange
        .update_state(
            1.into(),
            &bba!(QuoteCurrency::new(98, 0), QuoteCurrency::new(99, 0)),
        )
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fee0
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        QuoteCurrency::new(-198, 1)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(10, 0),
            position_margin: QuoteCurrency::new(990, 0),
            order_margin: QuoteCurrency::zero()
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee0);
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 1);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        1
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(
        exchange.account_tracker().buy_volume(),
        QuoteCurrency::new(990, 0)
    );
    assert_eq!(
        exchange.account_tracker().sell_volume(),
        QuoteCurrency::zero()
    );

    let sell_price = QuoteCurrency::new(105, 0);
    let fee1 = QuoteCurrency::convert_from(qty, sell_price) * *fee_maker.as_ref();
    let o = LimitOrder::new(Side::Sell, sell_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let order_updates = exchange
        .update_state(
            2.into(),
            &trade!(
                QuoteCurrency::new(106, 0),
                BaseCurrency::new(10, 0),
                Side::Buy
            ),
        )
        .unwrap();
    assert!(!order_updates.is_empty());
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(10495, 1) - fee0 - fee1,
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero()
        }
    );
    let order_updates = exchange
        .update_state(
            2.into(),
            &bba!(QuoteCurrency::new(106, 0), QuoteCurrency::new(107, 0)),
        )
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(10495, 1)
                - QuoteCurrency::new(198, 3)
                - QuoteCurrency::new(2079, 4),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero()
        }
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 2);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        2
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(
        exchange.account_tracker().buy_volume(),
        QuoteCurrency::new(990, 0)
    );
    assert_eq!(
        exchange.account_tracker().sell_volume(),
        QuoteCurrency::new(10395, 1)
    );
    assert_eq!(exchange.fees_paid(), QuoteCurrency::new(4059, 4));
}

#[test]
#[tracing_test::traced_test]
fn limit_orders_2() {
    let mut exchange = mock_exchange_linear();

    let exec_orders = exchange
        .update_state(
            0.into(),
            &Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
            },
        )
        .unwrap();
    assert!(exec_orders.is_empty());

    let o = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(101, 0),
        BaseCurrency::new(75, 2),
    )
    .unwrap();
    exchange.submit_limit_order(o).unwrap();

    let o = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 1),
    )
    .unwrap();
    exchange.submit_limit_order(o).unwrap();

    let exec_orders = exchange
        .update_state(
            1.into(),
            &trade!(
                QuoteCurrency::new(98, 0),
                BaseCurrency::new(2, 0),
                Side::Sell
            ),
        )
        .unwrap();
    let _ = exchange
        .update_state(
            1.into(),
            &bba!(QuoteCurrency::new(98, 0), QuoteCurrency::new(99, 0)),
        )
        .unwrap();
    assert_eq!(exec_orders.len(), 1);
}
