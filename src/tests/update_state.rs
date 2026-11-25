//! Tests for the boundary between the inline quote-only `update_state` path (flat account,
//! no pending stops, isolated margin, update cannot fill) and the full path.

use std::num::NonZeroU16;

use const_decimal::Decimal;

use crate::{
    DECIMALS,
    EXPECT_CONFIG,
    EXPECT_CONTRACT_SPEC,
    EXPECT_DECIMAL,
    EXPECT_NON_ZERO,
    EXPECT_QUANTITY_FILTER,
    mock_exchange_linear,
    prelude::*,
    utils::NoUserOrderId,
};

fn bba(bid: i64, ask: i64, ts: i64) -> Bba<i64, DECIMALS> {
    Bba {
        bid: QuoteCurrency::new(bid, 0),
        ask: QuoteCurrency::new(ask, 0),
        timestamp_exchange_ns: ts.into(),
    }
}

/// 5x leverage, so an isolated long can be liquidated by a moderate price drop.
fn levered_exchange(
    balance: i64,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::try_from_scaled(2, 4).expect(EXPECT_DECIMAL)),
        Fee::from(Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL)),
    )
    .expect(EXPECT_CONTRACT_SPEC);
    let config = Config::new(
        QuoteCurrency::new(balance, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    let mut ex = Exchange::new(config);
    ex.update_state(&bba(99, 100, 1)).unwrap();
    ex
}

#[test]
fn quote_only_update_of_flat_account_refreshes_market_state_and_keeps_orders() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&bba(100, 101, 1)).unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(90, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    let balances_before = ex.account().balances().clone();

    // The bid trades through the resting buy, but a `Bba` cannot fill it.
    let events = ex.update_state(&bba(80, 81, 2)).unwrap();
    assert!(events.is_empty());
    assert!(ex.stop_order_events().is_empty());
    assert_eq!(ex.market_state().bid(), QuoteCurrency::new(80, 0));
    assert_eq!(ex.market_state().ask(), QuoteCurrency::new(81, 0));
    assert_eq!(ex.market_state().current_timestamp_ns(), 2.into());
    assert_eq!(ex.account().active_limit_orders().len(), 1);
    assert!(ex.account().position().quantity().is_zero());
    assert_eq!(ex.account().balances(), &balances_before);
}

#[test]
fn quote_only_update_clears_the_events_of_the_previous_update() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&bba(100, 101, 1)).unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(102, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();

    // The fill closes the position, so the next update of this account is quote-only.
    let events = ex
        .update_state(&Trade {
            price: QuoteCurrency::new(103, 0),
            quantity: BaseCurrency::new(1, 0),
            side: Side::Buy,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert_eq!(events.len(), 1);
    assert!(ex.account().position().quantity().is_zero());

    assert!(ex.update_state(&bba(102, 103, 3)).unwrap().is_empty());
    assert!(ex.limit_order_events().is_empty());
}

#[test]
fn quote_update_liquidates_an_open_isolated_position() {
    let mut ex = levered_exchange(210);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    assert!(!ex.account().position().quantity().is_zero());

    assert_eq!(ex.update_state(&bba(70, 71, 2)), Err(RiskError::Liquidate));
    assert!(ex.account().position().quantity().is_zero());
}

#[test]
fn quote_update_with_open_position_above_liquidation_price_is_ok() {
    let mut ex = levered_exchange(210);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();

    assert!(ex.update_state(&bba(95, 96, 2)).unwrap().is_empty());
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(10, 0));
}

#[test]
fn quote_update_of_flat_account_still_elects_pending_stops() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&bba(100, 101, 1)).unwrap();
    ex.submit_stop_order(
        StopOrder::stop_market(
            Side::Buy,
            QuoteCurrency::new(110, 0),
            BaseCurrency::new(1, 0),
            NoUserOrderId,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(ex.account().position().quantity().is_zero());

    ex.update_state(&bba(115, 116, 2)).unwrap();
    assert_eq!(ex.stop_order_events().len(), 1);
    assert!(ex.pending_stops().is_empty());
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(1, 0));
}
