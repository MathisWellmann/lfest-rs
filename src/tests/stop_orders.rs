//! Tests for native conditional & stop orders: `StopMarket`, `StopLimit`,
//! `TakeProfitMarket`, `TakeProfitLimit`, `TrailingStop`, `StopProtection` collar,
//! `TriggerPriceType`, conservative candle sweeps, and `PortfolioExchange` stops.

use std::num::NonZeroU16;

use const_decimal::Decimal;

use crate::{
    DECIMALS,
    EXPECT_CONTRACT_SPEC,
    EXPECT_DECIMAL,
    EXPECT_NON_ZERO,
    EXPECT_QUANTITY_FILTER,
    mock_exchange_linear,
    prelude::*,
    utils::NoUserOrderId,
};

#[test]
fn stop_market_protects_short_intrabar_and_rejects_immediate_trigger() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();

    // Open short 2 @ 100.
    ex.submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    // Buy stop at 95 would trigger immediately (mark=100.5 >= 95) -> rejected!
    assert_eq!(
        ex.submit_stop_order(
            StopOrder::stop_market(
                Side::Buy,
                QuoteCurrency::new(95, 0),
                BaseCurrency::new(2, 0),
                NoUserOrderId,
            )
            .unwrap()
        )
        .unwrap_err(),
        StopOrderError::StopWouldTrigger
    );

    // Place reduce-only protective buy stop-loss at 135 (+35% short stop).
    let sl = ex
        .submit_stop_order(
            StopOrder::stop_market(
                Side::Buy,
                QuoteCurrency::new(135, 0),
                BaseCurrency::new(5, 0), // oversized, should clamp to 2 because reduce_only
                NoUserOrderId,
            )
            .unwrap()
            .with_reduce_only(true),
        )
        .unwrap();
    assert_eq!(ex.pending_stops().len(), 1);

    // Intrabar candle spikes to high = 140, closes at bid=120/ask=121 (below 135).
    // Stop MUST still trigger intrabar and fill conservatively at max(stop=135, ask=121) = 135!
    let candle = Candle::new(
        QuoteCurrency::new(120, 0),
        QuoteCurrency::new(121, 0),
        QuoteCurrency::new(99, 0),
        QuoteCurrency::new(140, 0),
        2.into(),
    )
    .unwrap();
    ex.update_state(&candle).unwrap();

    assert!(ex.pending_stops().is_empty());
    assert!(ex.account().position().quantity().is_zero());
    assert_eq!(ex.stop_order_events().len(), 1);
    match &ex.stop_order_events()[0] {
        StopOrderEvent::Filled {
            stop_order_id,
            filled_quantity,
            trigger_price,
            fill_price,
            ..
        } => {
            assert_eq!(*stop_order_id, sl.id());
            assert_eq!(*filled_quantity, BaseCurrency::new(2, 0));
            assert_eq!(*trigger_price, QuoteCurrency::new(140, 0));
            assert_eq!(*fill_price, QuoteCurrency::new(135, 0));
        }
        other => panic!("expected stop fill, got {other:?}"),
    }
}

#[test]
fn trailing_stop_ratchets_monotonically_and_fires_on_pullback() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    // Sell trailing stop with watermark 100 and absolute trail 5 -> initial stop at 95.
    let ts = ex
        .submit_stop_order(
            StopOrder::trailing_stop(
                Side::Sell,
                QuoteCurrency::new(100, 0),
                TrailDistance::Absolute(QuoteCurrency::new(5, 0)),
                BaseCurrency::new(2, 0),
                NoUserOrderId,
            )
            .unwrap()
            .with_trigger_price_type(TriggerPriceType::Last)
            .with_reduce_only(true),
        )
        .unwrap();
    assert_eq!(ts.stop_price(), QuoteCurrency::new(95, 0));

    // Trade rallies to 110 -> watermark becomes 110, stop ratchets up to 105 (not triggered).
    ex.update_state(&Trade {
        timestamp_exchange_ns: 2.into(),
        price: QuoteCurrency::new(110, 0),
        quantity: BaseCurrency::new(1, 0),
        side: Side::Buy,
    })
    .unwrap();
    assert_eq!(
        ex.pending_stops().get_by_id(ts.id()).unwrap().stop_price(),
        QuoteCurrency::new(105, 0)
    );

    // Pullback to 107 -> stop stays at 105 (monotonic, never loosens).
    ex.update_state(&Trade {
        timestamp_exchange_ns: 3.into(),
        price: QuoteCurrency::new(107, 0),
        quantity: BaseCurrency::new(1, 0),
        side: Side::Sell,
    })
    .unwrap();
    assert_eq!(
        ex.pending_stops().get_by_id(ts.id()).unwrap().stop_price(),
        QuoteCurrency::new(105, 0)
    );

    // Update BBA and trade prints at 105 -> triggers trailing stop and closes long!
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(104, 0),
        ask: QuoteCurrency::new(105, 0),
        timestamp_exchange_ns: 4.into(),
    })
    .unwrap();
    ex.update_state(&Trade {
        timestamp_exchange_ns: 5.into(),
        price: QuoteCurrency::new(105, 0),
        quantity: BaseCurrency::new(1, 0),
        side: Side::Sell,
    })
    .unwrap();
    assert!(ex.pending_stops().is_empty());
    assert!(ex.account().position().quantity().is_zero());
    assert!(matches!(
        ex.stop_order_events().first(),
        Some(StopOrderEvent::Filled { .. })
    ));
}

#[test]
fn stop_protection_collar_cancels_gapped_stop_and_stop_limit_places_child() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    // Sell stop at 95 with 3-unit protection collar (floor = 92).
    let collared = ex
        .submit_stop_order(
            StopOrder::stop_market(
                Side::Sell,
                QuoteCurrency::new(95, 0),
                BaseCurrency::new(1, 0),
                NoUserOrderId,
            )
            .unwrap()
            .with_protection(StopProtection::new(QuoteCurrency::new(3, 0)).unwrap())
            .with_reduce_only(true),
        )
        .unwrap();

    // Also place a TakeProfitLimit sell at stop=110, limit=112 (passive GTC).
    let tp_limit = ex
        .submit_stop_order(
            StopOrder::take_profit_limit(
                Side::Sell,
                QuoteCurrency::new(110, 0),
                QuoteCurrency::new(112, 0),
                BaseCurrency::new(1, 0),
                TimeInForce::GoodTilCancelled,
                NoUserOrderId,
            )
            .unwrap()
            .with_reduce_only(true),
        )
        .unwrap();

    // Market gaps down to bid=85/ask=86 (below collar floor 92) -> collared stop cancels!
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(85, 0),
        ask: QuoteCurrency::new(86, 0),
        timestamp_exchange_ns: 2.into(),
    })
    .unwrap();
    assert_eq!(
        ex.stop_order_events(),
        &[StopOrderEvent::Cancelled {
            stop_order_id: collared.id(),
            user_order_id: NoUserOrderId,
            reason: StopCancelReason::StopProtectionBand,
        }]
    );
    // Position remains open because collar prevented ruinous fill.
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(2, 0));

    // Market rebounds to bid=110/ask=111 -> TakeProfitLimit triggers and rests sell limit at 112.
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(110, 0),
        ask: QuoteCurrency::new(111, 0),
        timestamp_exchange_ns: 3.into(),
    })
    .unwrap();
    assert_eq!(ex.stop_order_events().len(), 1);
    match &ex.stop_order_events()[0] {
        StopOrderEvent::PlacedLimit {
            stop_order_id,
            child_order_id,
            ..
        } => {
            assert_eq!(*stop_order_id, tp_limit.id());
            assert!(
                ex.account()
                    .active_limit_orders()
                    .get_by_id(*child_order_id, Side::Sell)
                    .is_some()
            );
        }
        other => panic!("expected PlacedLimit event, got {other:?}"),
    }
}

#[test]
fn portfolio_exchange_stop_orders_fire_per_market() {
    let spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_ticker("SOLUSDT");
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            spec,
            OrderRateLimits::default(),
        )],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    let sol = MarketId::new(0);
    pf.update_market(
        sol,
        &Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();

    pf.submit_market_order(
        sol,
        MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    let stop = pf
        .submit_stop_order(
            sol,
            StopOrder::stop_market(
                Side::Buy,
                QuoteCurrency::new(120, 0),
                BaseCurrency::new(2, 0),
                NoUserOrderId,
            )
            .unwrap()
            .with_reduce_only(true),
        )
        .unwrap();

    pf.update_market(
        sol,
        &Bba {
            bid: QuoteCurrency::new(121, 0),
            ask: QuoteCurrency::new(122, 0),
            timestamp_exchange_ns: 2.into(),
        },
    )
    .unwrap();

    assert!(pf.market(sol).unwrap().position.quantity().is_zero());
    assert_eq!(pf.stop_events().len(), 1);
    assert_eq!(pf.stop_events()[0].market_id, sol);
    match &pf.stop_events()[0].event {
        StopOrderEvent::Filled {
            stop_order_id,
            fill_price,
            ..
        } => {
            assert_eq!(*stop_order_id, stop.id());
            assert_eq!(*fill_price, QuoteCurrency::new(122, 0));
        }
        other => panic!("expected portfolio stop fill, got {other:?}"),
    }
}
