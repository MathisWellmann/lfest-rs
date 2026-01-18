//! Test file for the linear futures mode of the exchange

#![allow(
    unused_crate_dependencies,
    reason = "Integration tests don't use all dev dependencies"
)]

use lfest::{
    mock_exchange_linear_with_account_tracker,
    prelude::*,
    test_fee_taker,
};

#[test]
#[tracing_test::traced_test]
fn lin_long_market_win_full() {
    let starting_balance = QuoteCurrency::new(1000, 0);
    let mut exchange = mock_exchange_linear_with_account_tracker(starting_balance);
    let _ = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert_eq!(
        exchange.account().balances().total_fees_paid(),
        QuoteCurrency::zero()
    );

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

    let fee = QuoteCurrency::convert_from(qty, bid) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.account().position().clone(),
        Position::Long(PositionInner::new(qty, bid))
    );
    assert_eq!(
        exchange.account().position().unrealized_pnl(bid, ask),
        QuoteCurrency::zero()
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(500, 0) - fee)
            .position_margin(QuoteCurrency::new(500, 0))
            .total_fees_paid(fee)
            .build()
    );
    assert_eq!(
        exchange
            .account()
            .order_margin(exchange.config().contract_spec().init_margin_req()),
        Zero::zero()
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
            .account()
            .position()
            .unrealized_pnl(QuoteCurrency::new(200, 0), QuoteCurrency::new(201, 0)),
        QuoteCurrency::new(500, 0)
    );

    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0)).unwrap())
        .unwrap();

    assert_eq!(exchange.account().position(), &Position::Neutral);
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(QuoteCurrency::new(200, 0), QuoteCurrency::new(201, 0)),
        QuoteCurrency::zero()
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(14991, 1))
            .position_margin(QuoteCurrency::zero())
            .total_fees_paid(QuoteCurrency::new(9, 1))
            .build()
    );
    assert_eq!(
        exchange
            .account()
            .order_margin(exchange.config().contract_spec().init_margin_req()),
        Zero::zero()
    );
    assert_eq!(
        exchange.account().balances().total_fees_paid(),
        QuoteCurrency::new(9, 1)
    );
}
