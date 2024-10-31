//! Test file for the linear futures mode of the exchange

use lfest::{mock_exchange_linear_with_account_tracker, prelude::*, test_fee_taker};

#[test]
#[tracing_test::traced_test]
fn lin_long_market_win_full() {
    let starting_balance = QuoteCurrency::new(1000, 0);
    let mut exchange = mock_exchange_linear_with_account_tracker(starting_balance);
    let mut accounting = InMemoryTransactionAccounting::new(starting_balance);
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let _ = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 0);
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

    let qty = BaseCurrency::new(5, 0);
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, qty).unwrap())
        .unwrap();
    let bid = QuoteCurrency::new(100, 0);
    let ask = QuoteCurrency::new(101, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 0);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 1);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 1);
    assert_eq!(
        exchange.account_tracker().buy_volume(),
        QuoteCurrency::new(500, 0)
    );
    assert_eq!(
        exchange.account_tracker().sell_volume(),
        QuoteCurrency::zero()
    );

    let fees = QuoteCurrency::convert_from(qty, bid) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            bid,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        QuoteCurrency::zero()
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(500, 0),
            position_margin: QuoteCurrency::new(500, 0),
            order_margin: QuoteCurrency::zero(),
            _q: std::marker::PhantomData
        }
    );

    let bid = QuoteCurrency::new(200, 0);
    let ask = QuoteCurrency::new(201, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(200, 0), QuoteCurrency::new(201, 0)),
        QuoteCurrency::new(500, 0)
    );

    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0)).unwrap())
        .unwrap();

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(200, 0), QuoteCurrency::new(201, 0)),
        QuoteCurrency::zero()
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(14991, 1),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero(),
            _q: std::marker::PhantomData
        }
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 0);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 2);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 2);
    assert_eq!(
        exchange.account_tracker().buy_volume(),
        QuoteCurrency::new(500, 0)
    );
    assert_eq!(
        exchange.account_tracker().sell_volume(),
        QuoteCurrency::new(1000, 0)
    );
    assert_eq!(exchange.fees_paid(), QuoteCurrency::new(9, 1));
}
