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
            // Only `Last` stops watch the intrabar trade range of a candle.
            .with_trigger_price_type(TriggerPriceType::Last)
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

/// Long 2 with a resting reduce-only take-profit; a stop closes the position, so the
/// venue force-cancels the stale reduce-only order during the stop execution.
fn exchange_with_stale_reduce_only_after_stop(
    stop: StopOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>,
) -> (
    Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>,
    OrderId,
) {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    let tp = ex
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(110, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap()
            .with_reduce_only(true),
        )
        .unwrap();
    ex.submit_stop_order(stop).unwrap();
    (ex, tp.id())
}

#[test]
fn forced_cancel_from_elected_market_stop_survives_update_state() {
    let stop = StopOrder::stop_market(
        Side::Sell,
        QuoteCurrency::new(95, 0),
        BaseCurrency::new(2, 0),
        NoUserOrderId,
    )
    .unwrap();
    let (mut ex, tp_id) = exchange_with_stale_reduce_only_after_stop(stop);

    let events = ex
        .update_state(&Bba {
            bid: QuoteCurrency::new(94, 0),
            ask: QuoteCurrency::new(95, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap()
        .clone();
    assert!(ex.account().position().quantity().is_zero());
    assert!(ex.account().active_limit_orders().is_empty());
    assert!(matches!(
        ex.stop_order_events().as_slice(),
        [StopOrderEvent::Filled { .. }]
    ));
    assert_eq!(events.len(), 1);
    match &events[0] {
        LimitOrderEvent::ForcedCancel(order) => assert_eq!(order.id(), tp_id),
        other => panic!("expected forced cancel, got {other:?}"),
    }
}

#[test]
fn forced_cancel_from_immediately_filled_stop_limit_child_is_reported() {
    // Marketable child (limit 90 below the bid) fills immediately on election.
    let stop = StopOrder::stop_limit(
        Side::Sell,
        QuoteCurrency::new(95, 0),
        QuoteCurrency::new(90, 0),
        BaseCurrency::new(2, 0),
        TimeInForce::GoodTilCancelled,
        NoUserOrderId,
    )
    .unwrap();
    let (mut ex, tp_id) = exchange_with_stale_reduce_only_after_stop(stop);

    let events = ex
        .update_state(&Bba {
            bid: QuoteCurrency::new(94, 0),
            ask: QuoteCurrency::new(95, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap()
        .clone();
    assert!(ex.account().position().quantity().is_zero());
    assert!(ex.account().active_limit_orders().is_empty());
    assert!(matches!(
        ex.stop_order_events().as_slice(),
        [StopOrderEvent::Filled { .. }]
    ));
    assert_eq!(events.len(), 1);
    match &events[0] {
        LimitOrderEvent::ForcedCancel(order) => assert_eq!(order.id(), tp_id),
        other => panic!("expected forced cancel, got {other:?}"),
    }
}

/// 10x leverage, 5% maintenance margin, no fees.
fn levered_spec() -> ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    ContractSpecification::new(
        leverage!(10),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
}

/// Two sell stops below a long 10 @ 100; the candle elects both and the first fill
/// (5 @ 80, realizing -100) liquidates the account.
fn cascade_stops() -> [StopOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>; 2] {
    [
        StopOrder::stop_market(
            Side::Sell,
            QuoteCurrency::new(95, 0),
            BaseCurrency::new(5, 0),
            NoUserOrderId,
        )
        .unwrap(),
        StopOrder::stop_market(
            Side::Sell,
            QuoteCurrency::new(90, 0),
            BaseCurrency::new(1, 0),
            NoUserOrderId,
        )
        .unwrap(),
    ]
}

fn crash_candle() -> Candle<i64, DECIMALS> {
    Candle::new(
        QuoteCurrency::new(80, 0),
        QuoteCurrency::new(81, 0),
        QuoteCurrency::new(79, 0),
        QuoteCurrency::new(101, 0),
        2.into(),
    )
    .unwrap()
}

#[test]
fn liquidation_mid_stop_cascade_reports_remaining_elected_stops() {
    let config = Config::new(
        QuoteCurrency::new(110, 0),
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        levered_spec(),
        OrderRateLimits::default(),
    )
    .unwrap();
    let mut ex = Exchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(config);
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(99, 0),
        ask: QuoteCurrency::new(100, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    let [first, second] = cascade_stops().map(|s| ex.submit_stop_order(s).unwrap());

    assert_eq!(
        ex.update_state(&crash_candle()).unwrap_err(),
        RiskError::Liquidate
    );
    assert!(ex.account().position().quantity().is_zero());
    assert!(ex.pending_stops().is_empty());
    let events = ex.stop_order_events();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(matches!(
        events[0],
        StopOrderEvent::Filled { stop_order_id, .. } if stop_order_id == first.id()
    ));
    assert_eq!(
        events[1],
        StopOrderEvent::Cancelled {
            stop_order_id: second.id(),
            user_order_id: NoUserOrderId,
            reason: StopCancelReason::ForcedCancel,
        }
    );
}

fn levered_portfolio(
    balance: QuoteCurrency<i64, DECIMALS>,
) -> PortfolioExchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let cfg = PortfolioConfig::new(
        balance,
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            levered_spec().with_ticker("BTCUSDT"),
            OrderRateLimits::default(),
        )],
    )
    .unwrap();
    let mut pf = PortfolioExchange::new(cfg);
    pf.update_market(
        MarketId::new(0),
        &Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();
    pf
}

#[test]
fn portfolio_liquidation_mid_stop_cascade_reports_remaining_elected_stops() {
    let mut pf = levered_portfolio(QuoteCurrency::new(110, 0));
    let m = MarketId::new(0);
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    let [first, second] = cascade_stops().map(|s| pf.submit_stop_order(m, s).unwrap());

    assert_eq!(
        pf.update_market(m, &crash_candle()).unwrap_err(),
        PortfolioError::Risk(RiskError::Liquidate)
    );
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
    assert!(pf.pending_stops(m).unwrap().is_empty());
    let events = pf.stop_events();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(matches!(
        events[0].event,
        StopOrderEvent::Filled { stop_order_id, .. } if stop_order_id == first.id()
    ));
    assert_eq!(
        events[1].event,
        StopOrderEvent::Cancelled {
            stop_order_id: second.id(),
            user_order_id: NoUserOrderId,
            reason: StopCancelReason::ForcedCancel,
        }
    );
}

#[test]
fn portfolio_forced_cancel_from_immediately_filled_stop_limit_child_is_reported() {
    let mut pf = levered_portfolio(QuoteCurrency::new(200, 0));
    let m = MarketId::new(0);
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    // Resting bid adding 50 of order margin: required collateral 150 <= equity 200.
    let bid = pf
        .submit_limit_order(
            m,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(50, 0),
                BaseCurrency::new(10, 0),
            )
            .unwrap(),
        )
        .unwrap();
    pf.submit_stop_order(
        m,
        StopOrder::stop_limit(
            Side::Sell,
            QuoteCurrency::new(95, 0),
            QuoteCurrency::new(90, 0),
            BaseCurrency::new(1, 0),
            TimeInForce::GoodTilCancelled,
            NoUserOrderId,
        )
        .unwrap(),
    )
    .unwrap();

    // The child sells 1 @ 92 (marketable); the unrealized loss leaves the equity below
    // the required collateral, so the venue force-cancels the resting bid.
    let events = pf
        .update_market(
            m,
            &Bba {
                bid: QuoteCurrency::new(92, 0),
                ask: QuoteCurrency::new(93, 0),
                timestamp_exchange_ns: 2.into(),
            },
        )
        .unwrap()
        .clone();
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(9, 0)
    );
    assert!(matches!(
        pf.stop_events()[0].event,
        StopOrderEvent::Filled { .. }
    ));
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].market_id, m);
    match &events[0].event {
        LimitOrderEvent::ForcedCancel(order) => assert_eq!(order.id(), bid.id()),
        other => panic!("expected forced cancel, got {other:?}"),
    }
}

/// Linear exchange with the mock contract, `max_orders` open orders (and pending stops)
/// and `orders_per_second` user order actions.
fn small_exchange(
    max_orders: u16,
    orders_per_second: u32,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(max_orders).expect(EXPECT_NON_ZERO),
        mock_exchange_linear().config().contract_spec().clone(),
        OrderRateLimits::new(std::num::NonZeroU32::new(orders_per_second).expect(EXPECT_NON_ZERO)),
    )
    .unwrap();
    let mut ex = Exchange::new(config);
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex
}

fn reduce_only_sell_stop(
    stop_price: i64,
) -> StopOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    StopOrder::stop_market(
        Side::Sell,
        QuoteCurrency::new(stop_price, 0),
        BaseCurrency::new(5, 0),
        NoUserOrderId,
    )
    .unwrap()
    .with_reduce_only(true)
}

fn sell_stop_limit_95_90() -> StopOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    StopOrder::stop_limit(
        Side::Sell,
        QuoteCurrency::new(95, 0),
        QuoteCurrency::new(90, 0),
        BaseCurrency::new(2, 0),
        TimeInForce::GoodTilCancelled,
        NoUserOrderId,
    )
    .unwrap()
    .with_reduce_only(true)
}

fn bba_94_95() -> Bba<i64, DECIMALS> {
    Bba {
        bid: QuoteCurrency::new(94, 0),
        ask: QuoteCurrency::new(95, 0),
        timestamp_exchange_ns: 2.into(),
    }
}

#[test]
fn elected_stop_limit_child_ignores_the_user_rate_limit() {
    // Two user actions per second: the market order and the stop use them up.
    let mut ex = small_exchange(10, 2);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    ex.submit_stop_order(sell_stop_limit_95_90()).unwrap();
    assert_eq!(
        ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
            .unwrap_err(),
        SubmitMarketOrderError::RateLimitReached(crate::order_rate_limiter::RateLimitReached)
    );

    // Same rate-limit bucket: the venue-placed child must still execute.
    ex.update_state(&bba_94_95()).unwrap();
    assert!(
        matches!(
            ex.stop_order_events().as_slice(),
            [StopOrderEvent::Filled { .. }]
        ),
        "{:?}",
        ex.stop_order_events()
    );
    assert!(ex.account().position().quantity().is_zero());
}

#[test]
fn portfolio_elected_stop_limit_child_ignores_the_user_rate_limit() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
            mock_exchange_linear().config().contract_spec().clone(),
            OrderRateLimits::new(std::num::NonZeroU32::new(2).expect(EXPECT_NON_ZERO)),
        )],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    let m = MarketId::new(0);
    pf.update_market(
        m,
        &Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_stop_order(m, sell_stop_limit_95_90()).unwrap();

    pf.update_market(m, &bba_94_95()).unwrap();
    assert_eq!(pf.stop_events().len(), 1, "{:?}", pf.stop_events());
    assert!(matches!(
        pf.stop_events()[0].event,
        StopOrderEvent::Filled { .. }
    ));
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
}

#[test]
fn liquidation_from_apply_funding_reports_every_stop_cancellation() {
    // Two pending stops at most, so two events fill the stop event buffer.
    let mut ex = small_exchange(2, 100);
    // Without a position both reduce-only stops are cancelled on election.
    ex.submit_stop_order(reduce_only_sell_stop(95)).unwrap();
    ex.submit_stop_order(reduce_only_sell_stop(95)).unwrap();
    ex.update_state(&bba_94_95()).unwrap();
    assert_eq!(ex.stop_order_events().len(), 2);

    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(5, 0)).unwrap())
        .unwrap();
    let s0 = ex.submit_stop_order(reduce_only_sell_stop(50)).unwrap();
    let s1 = ex.submit_stop_order(reduce_only_sell_stop(50)).unwrap();
    // Paying 800 of funding leaves the equity below the maintenance margin.
    let settlement = ex.apply_funding(
        FundingRate::from(Decimal::try_from_scaled(16, 1).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(100, 0),
    );
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert!(ex.pending_stops().is_empty());
    let cancelled: Vec<OrderId> = ex
        .stop_order_events()
        .iter()
        .map(|event| match event {
            StopOrderEvent::Cancelled {
                stop_order_id,
                reason: StopCancelReason::ForcedCancel,
                ..
            } => *stop_order_id,
            other => panic!("expected a forced stop cancel, got {other:?}"),
        })
        .collect();
    assert_eq!(cancelled, vec![s0.id(), s1.id()]);
}

#[test]
fn portfolio_liquidation_from_apply_funding_reports_only_its_own_stop_events() {
    let mut pf = levered_portfolio(QuoteCurrency::new(200, 0));
    let m = MarketId::new(0);
    pf.submit_stop_order(m, reduce_only_sell_stop(95)).unwrap();
    pf.submit_stop_order(m, reduce_only_sell_stop(95)).unwrap();
    pf.update_market(m, &bba_94_95()).unwrap();
    assert_eq!(pf.stop_events().len(), 2);

    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    let stop = pf.submit_stop_order(m, reduce_only_sell_stop(50)).unwrap();
    // Paying 190 of funding leaves the equity below the maintenance margin.
    let settlement = pf
        .apply_funding(
            m,
            FundingRate::from(Decimal::try_from_scaled(2, 1).expect(EXPECT_DECIMAL)),
            QuoteCurrency::new(95, 0),
        )
        .unwrap();
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert_eq!(pf.stop_events().len(), 1, "{:?}", pf.stop_events());
    assert_eq!(
        pf.stop_events()[0].event,
        StopOrderEvent::Cancelled {
            stop_order_id: stop.id(),
            user_order_id: NoUserOrderId,
            reason: StopCancelReason::ForcedCancel,
        }
    );
}

#[test]
fn cloned_exchange_reports_every_stop_cancelled_by_a_liquidation() {
    let mut ex = small_exchange(2, 100);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(5, 0)).unwrap())
        .unwrap();
    for _ in 0..2 {
        ex.submit_stop_order(reduce_only_sell_stop(50)).unwrap();
    }
    // `Vec::clone` drops spare capacity, so the clone starts with an empty event buffer.
    let mut cloned = ex.clone();
    let settlement = cloned.apply_funding(
        FundingRate::from(Decimal::try_from_scaled(16, 1).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(100, 0),
    );
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert_eq!(cloned.stop_order_events().len(), 2);
}

#[test]
fn cloned_portfolio_reports_every_stop_cancelled_by_a_liquidation() {
    let mut pf = levered_portfolio(QuoteCurrency::new(200, 0));
    let m = MarketId::new(0);
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    for _ in 0..2 {
        pf.submit_stop_order(m, reduce_only_sell_stop(50)).unwrap();
    }
    let mut cloned = pf.clone();
    let settlement = cloned
        .apply_funding(
            m,
            FundingRate::from(Decimal::try_from_scaled(2, 1).expect(EXPECT_DECIMAL)),
            QuoteCurrency::new(95, 0),
        )
        .unwrap();
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert_eq!(cloned.stop_events().len(), 2);
}

/// Linear mock contract marked at an external fair price of 100, quoted 100 / 101,
/// with a long 2 @ 101 open.
fn fair_price_exchange_long_2()
-> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let spec = mock_exchange_linear()
        .config()
        .contract_spec()
        .clone()
        .with_mark_method(MarkMethod::FairPrice);
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    let mut ex = Exchange::new(config);
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.update_state(&MarkPriceUpdate::new(QuoteCurrency::new(100, 0), 2.into()))
        .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    ex
}

fn sell_stop_1(
    stop_price: i64,
    price_type: TriggerPriceType,
) -> StopOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    StopOrder::stop_market(
        Side::Sell,
        QuoteCurrency::new(stop_price, 0),
        BaseCurrency::new(1, 0),
        NoUserOrderId,
    )
    .unwrap()
    .with_trigger_price_type(price_type)
    .with_reduce_only(true)
}

/// A candle whose trades wick down to 80 while the quotes close at 99 / 100.
fn wick_candle() -> Candle<i64, DECIMALS> {
    Candle::new(
        QuoteCurrency::new(99, 0),
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(80, 0),
        QuoteCurrency::new(101, 0),
        3.into(),
    )
    .unwrap()
}

fn filled_stop_id<I, const D: u8, B, U>(event: &StopOrderEvent<I, D, B, U>) -> Option<OrderId>
where
    I: Mon<D>,
    B: Currency<I, D>,
    B::PairedCurrency: MarginCurrency<I, D>,
    U: UserOrderId,
{
    match event {
        StopOrderEvent::Filled { stop_order_id, .. } => Some(*stop_order_id),
        _ => None,
    }
}

#[test]
fn fair_price_mark_stop_ignores_a_trade_wick() {
    let mut ex = fair_price_exchange_long_2();
    let mark_stop = ex
        .submit_stop_order(sell_stop_1(95, TriggerPriceType::Mark))
        .unwrap();
    let last_stop = ex
        .submit_stop_order(sell_stop_1(95, TriggerPriceType::Last))
        .unwrap();

    // The wick prints trades at 80, but the fair price stays at 100: only the stop
    // watching trades may fire.
    ex.update_state(&wick_candle()).unwrap();
    let events = ex.stop_order_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(filled_stop_id(&events[0]), Some(last_stop.id()));
    assert!(ex.pending_stops().get_by_id(mark_stop.id()).is_some());
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(1, 0));

    // The mark itself falls through the stop.
    ex.update_state(&MarkPriceUpdate::new(QuoteCurrency::new(94, 0), 4.into()))
        .unwrap();
    let events = ex.stop_order_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(filled_stop_id(&events[0]), Some(mark_stop.id()));
    match &events[0] {
        StopOrderEvent::Filled { trigger_price, .. } => {
            assert_eq!(*trigger_price, QuoteCurrency::new(94, 0));
        }
        other => panic!("expected fill, got {other:?}"),
    }
    assert!(ex.account().position().quantity().is_zero());
}

#[test]
fn portfolio_fair_price_mark_stop_ignores_a_trade_wick() {
    let spec = levered_spec()
        .with_ticker("BTCUSDT")
        .with_mark_method(MarkMethod::FairPrice);
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
    let m = MarketId::new(0);
    pf.update_market(
        m,
        &Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();
    pf.update_market(
        m,
        &MarkPriceUpdate::new(QuoteCurrency::new(100, 0), 2.into()),
    )
    .unwrap();
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    let mark_stop = pf
        .submit_stop_order(m, sell_stop_1(95, TriggerPriceType::Mark))
        .unwrap();
    let last_stop = pf
        .submit_stop_order(m, sell_stop_1(95, TriggerPriceType::Last))
        .unwrap();

    pf.update_market(m, &wick_candle()).unwrap();
    let events = pf.stop_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(filled_stop_id(&events[0].event), Some(last_stop.id()));
    assert!(
        pf.pending_stops(m)
            .unwrap()
            .get_by_id(mark_stop.id())
            .is_some()
    );

    pf.update_market(
        m,
        &MarkPriceUpdate::new(QuoteCurrency::new(94, 0), 4.into()),
    )
    .unwrap();
    let events = pf.stop_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(filled_stop_id(&events[0].event), Some(mark_stop.id()));
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
}

#[test]
fn best_bid_ask_stop_watches_the_quotes_not_the_trades() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    let stop = ex
        .submit_stop_order(sell_stop_1(95, TriggerPriceType::BestBidAsk))
        .unwrap();

    // Trades wick to 80 but the bid closes at 99, above the stop.
    ex.update_state(&wick_candle()).unwrap();
    assert!(ex.stop_order_events().is_empty());
    assert_eq!(ex.pending_stops().len(), 1);

    // The ask touching 95 is not the sell stop's feed either.
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(96, 0),
        ask: QuoteCurrency::new(97, 0),
        timestamp_exchange_ns: 4.into(),
    })
    .unwrap();
    assert!(ex.stop_order_events().is_empty());

    // The bid reaching the stop elects it at the bid.
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(95, 0),
        ask: QuoteCurrency::new(96, 0),
        timestamp_exchange_ns: 5.into(),
    })
    .unwrap();
    let events = ex.stop_order_events();
    assert_eq!(events.len(), 1, "{events:?}");
    match &events[0] {
        StopOrderEvent::Filled {
            stop_order_id,
            trigger_price,
            fill_price,
            ..
        } => {
            assert_eq!(*stop_order_id, stop.id());
            assert_eq!(*trigger_price, QuoteCurrency::new(95, 0));
            assert_eq!(*fill_price, QuoteCurrency::new(95, 0));
        }
        other => panic!("expected fill, got {other:?}"),
    }
}

/// Sell trail 5 from watermark 100 (stop 95). A bar with high 110 and low 101 may have
/// rallied to 110 first (stop 105) and then fallen to 101, so it must elect the stop.
#[test]
fn trailing_stop_on_a_candle_trails_before_testing_the_adverse_extreme() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
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

    let bar = Candle::new(
        QuoteCurrency::new(106, 0),
        QuoteCurrency::new(107, 0),
        QuoteCurrency::new(101, 0),
        QuoteCurrency::new(110, 0),
        2.into(),
    )
    .unwrap();
    ex.update_state(&bar).unwrap();
    assert!(ex.pending_stops().is_empty());
    assert!(ex.account().position().quantity().is_zero());
    let events = ex.stop_order_events();
    assert_eq!(events.len(), 1, "{events:?}");
    match &events[0] {
        StopOrderEvent::Filled {
            stop_order_id,
            trigger_price,
            fill_price,
            ..
        } => {
            assert_eq!(*stop_order_id, ts.id());
            assert_eq!(*trigger_price, QuoteCurrency::new(101, 0));
            // The worse of the trailed stop (105) and the closing bid (106).
            assert_eq!(*fill_price, QuoteCurrency::new(105, 0));
        }
        other => panic!("expected fill, got {other:?}"),
    }
}

#[test]
fn cloned_exchange_accepts_new_stop_orders() {
    // A derived `Clone` kept only the length of the pre-allocated stop store, so admission
    // failed with `MaxPendingStops(0)`. A clone must behave like a fresh exchange.
    let ex = small_exchange(10, 1_000);
    let mut cloned = ex.clone();
    cloned
        .submit_stop_order(
            StopOrder::stop_market(
                Side::Sell,
                QuoteCurrency::new(95, 0),
                BaseCurrency::new(1, 0),
                NoUserOrderId,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(cloned.pending_stops().len(), 1);
}

#[test]
fn cloned_exchange_reports_a_triggered_stop() {
    // Election pushes into the elected-stop and stop-event scratch buffers, which a
    // derived `Clone` left without spare capacity.
    let mut ex = small_exchange(10, 1_000);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    ex.submit_stop_order(reduce_only_sell_stop(95)).unwrap();

    let mut cloned = ex.clone();
    cloned.update_state(&bba_94_95()).unwrap();

    assert!(cloned.pending_stops().is_empty());
    assert!(cloned.account().position().quantity().is_zero());
    let events = cloned.stop_order_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(matches!(events[0], StopOrderEvent::Filled { .. }));
}

#[test]
fn cloned_portfolio_accepts_new_stop_orders() {
    // A portfolio clone shrinks each market's pre-allocated stop store to its length, so
    // admission must not fail with `MaxPendingStops(0)`.
    let pf = levered_portfolio(QuoteCurrency::new(200, 0));
    let mut cloned = pf.clone();
    cloned
        .submit_stop_order(
            MarketId::new(0),
            StopOrder::stop_market(
                Side::Sell,
                QuoteCurrency::new(95, 0),
                BaseCurrency::new(1, 0),
                NoUserOrderId,
            )
            .unwrap(),
        )
        .unwrap();
    let stops = cloned
        .pending_stops(MarketId::new(0))
        .expect("market exists");
    assert_eq!(stops.len(), 1);
}
