//! Tests for `TimeInForce` (`GTX`, `GTC`, `IOC`, `FOK`), `RePricing::Marketable`,
//! and `reduce_only` orders on both `Exchange` and `PortfolioExchange`.

use std::num::NonZeroU16;

use const_decimal::Decimal;

use super::bba;
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
fn marketable_gtc_ioc_fok_limit_order_execution_and_rejection() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();

    // Non-crossing IOC and FOK are rejected without resting.
    for tif in [TimeInForce::ImmediateOrCancel, TimeInForce::FillOrKill] {
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap()
        .with_time_in_force(tif);
        assert_eq!(
            ex.submit_limit_order_with_execution(order).unwrap_err(),
            SubmitLimitOrderError::ImmediateExecutionNotMarketable
        );
    }
    assert!(ex.account().active_limit_orders().is_empty());

    // Non-crossing GTC rests passively on the book.
    let gtc_passive = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(1, 0),
    )
    .unwrap()
    .with_time_in_force(TimeInForce::GoodTilCancelled);
    let sub = ex.submit_limit_order_with_execution(gtc_passive).unwrap();
    assert!(matches!(sub, LimitOrderSubmission::Resting(_)));
    assert_eq!(ex.account().active_limit_orders().len(), 1);

    // Crossing GTC buy at 105 executes immediately at best ask = 101 paying taker fee.
    let gtc_cross = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(105, 0),
        BaseCurrency::new(2, 0),
    )
    .unwrap()
    .with_time_in_force(TimeInForce::GoodTilCancelled);
    let sub = ex.submit_limit_order_with_execution(gtc_cross).unwrap();
    match sub {
        LimitOrderSubmission::ImmediatelyFilled {
            filled_order,
            fee,
            solvency,
            ..
        } => {
            assert_eq!(
                filled_order.state().avg_fill_price(),
                QuoteCurrency::new(101, 0)
            );
            assert_eq!(filled_order.total_quantity(), BaseCurrency::new(2, 0));
            assert!(fee > QuoteCurrency::zero());
            assert_eq!(solvency, Solvency::Solvent);
        }
        LimitOrderSubmission::Resting(_) => panic!("expected immediate marketable execution"),
    }
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(2, 0));

    // Crossing IOC sell at 99 closes 1 unit at best bid = 100.
    let ioc_cross = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(1, 0),
    )
    .unwrap()
    .with_time_in_force(TimeInForce::ImmediateOrCancel);
    let sub = ex.submit_limit_order_with_execution(ioc_cross).unwrap();
    assert!(matches!(
        sub,
        LimitOrderSubmission::ImmediatelyFilled { .. }
    ));
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(1, 0));
}

#[test]
fn reduce_only_clamps_excess_rejects_flat_and_prunes_stale_resting_orders() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();

    // Flat position rejects reduce-only market and limit orders.
    assert_eq!(
        ex.submit_market_order(
            MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0))
                .unwrap()
                .with_reduce_only(true)
        )
        .unwrap_err(),
        SubmitMarketOrderError::ReduceOnlyRejected
    );
    assert_eq!(
        ex.submit_limit_order_with_execution(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(105, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap()
            .with_reduce_only(true)
        )
        .unwrap_err(),
        SubmitLimitOrderError::ReduceOnlyRejected
    );

    // Open long 2 @ 101.
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    // Rest a reduce-only sell limit for qty 5 -> clamped to open position qty 2.
    let sub = ex
        .submit_limit_order_with_execution(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(110, 0),
                BaseCurrency::new(5, 0),
            )
            .unwrap()
            .with_reduce_only(true),
        )
        .unwrap();
    let LimitOrderSubmission::Resting(ro_limit) = sub else {
        panic!("expected passive resting reduce-only limit");
    };
    assert_eq!(ro_limit.remaining_quantity(), BaseCurrency::new(2, 0));
    assert!(ro_limit.reduce_only());

    // Submit a reduce-only sell market order for qty 10 -> clamped to 2 (closes long, never flips short).
    // Closing the long also prunes the now-stale resting reduce-only sell limit!
    let settlement = ex
        .submit_market_order(
            MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0))
                .unwrap()
                .with_reduce_only(true),
        )
        .unwrap();
    assert_eq!(settlement.filled_order.quantity(), BaseCurrency::new(2, 0));
    assert!(ex.account().position().quantity().is_zero());
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0].id(), ro_limit.id());
    assert!(ex.account().active_limit_orders().is_empty());
}

/// A reduce-only buy can only shrink a short. Flat and long positions have nothing to
/// reduce, so it is rejected instead of being clamped to the position size.
#[test]
fn reduce_only_buy_is_rejected_while_flat_or_long() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&bba(100, 101, 1)).unwrap();
    let ro_market_buy = || {
        MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0))
            .unwrap()
            .with_reduce_only(true)
    };
    let ro_limit_buy = || {
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(95, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap()
        .with_reduce_only(true)
    };
    assert_eq!(
        ex.submit_market_order(ro_market_buy()).unwrap_err(),
        SubmitMarketOrderError::ReduceOnlyRejected
    );
    assert_eq!(
        ex.submit_limit_order(ro_limit_buy()).unwrap_err(),
        SubmitLimitOrderError::ReduceOnlyRejected
    );

    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    assert_eq!(
        ex.submit_market_order(ro_market_buy()).unwrap_err(),
        SubmitMarketOrderError::ReduceOnlyRejected
    );
    assert_eq!(
        ex.submit_limit_order(ro_limit_buy()).unwrap_err(),
        SubmitLimitOrderError::ReduceOnlyRejected
    );
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(2, 0));
    assert!(ex.account().active_limit_orders().is_empty());
}

#[test]
fn portfolio_exchange_supports_marketable_tif_and_reduce_only() {
    let spec = ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_ticker("BTCUSDT");
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
    let btc = MarketId::new(0);
    pf.update_market(
        btc,
        &Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();

    // Crossing FOK buy at 102 fills immediately at 101.
    let sub = pf
        .submit_limit_order_with_execution(
            btc,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(102, 0),
                BaseCurrency::new(3, 0),
            )
            .unwrap()
            .with_time_in_force(TimeInForce::FillOrKill),
        )
        .unwrap();
    assert!(matches!(
        sub,
        LimitOrderSubmission::ImmediatelyFilled { .. }
    ));
    assert_eq!(
        pf.market(btc).unwrap().position.quantity(),
        BaseCurrency::new(3, 0)
    );

    // Reduce-only sell market of qty 5 clamps to 3.
    let mkt = pf
        .submit_market_order(
            btc,
            MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0))
                .unwrap()
                .with_reduce_only(true),
        )
        .unwrap();
    assert_eq!(mkt.filled_order.quantity(), BaseCurrency::new(3, 0));
    assert!(pf.market(btc).unwrap().position.quantity().is_zero());
}

fn reduce_only_sell(
    price: i64,
    qty: i64,
) -> LimitOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder> {
    LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(price, 0),
        BaseCurrency::new(qty, 0),
    )
    .unwrap()
    .with_reduce_only(true)
}

/// A candle trading through every resting ask up to 120.
fn rally_candle() -> Candle<i64, DECIMALS> {
    Candle::new(
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(101, 0),
        QuoteCurrency::new(99, 0),
        QuoteCurrency::new(120, 0),
        2.into(),
    )
    .unwrap()
}

fn long_10_exchange() -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let mut ex = crate::mock_exchange_linear_with_account_tracker(QuoteCurrency::new(10_000, 0));
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    ex
}

#[test]
fn resting_reduce_only_order_never_flips_position_after_partial_close() {
    let mut ex = long_10_exchange();
    let ro = ex.submit_limit_order(reduce_only_sell(110, 10)).unwrap();
    assert_eq!(ro.remaining_quantity(), BaseCurrency::new(10, 0));

    // Market sell 6 -> long 4; the resting reduce-only sell 10 no longer fits.
    let settlement = ex
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(6, 0)).unwrap())
        .unwrap();
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(4, 0));
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0].id(), ro.id());
    assert!(ex.account().active_limit_orders().is_empty());
    assert!(ex.account().order_margin().is_zero());

    // A rally through 110 must not flip the account short.
    assert!(ex.update_state(&rally_candle()).unwrap().is_empty());
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(4, 0));
}

#[test]
fn resting_reduce_only_orders_are_capped_by_position_in_total() {
    let mut ex = long_10_exchange();
    let near = ex.submit_limit_order(reduce_only_sell(105, 4)).unwrap();
    // Only 6 of the position remain uncovered, so the second order is clamped to 6.
    let far = ex.submit_limit_order(reduce_only_sell(110, 8)).unwrap();
    assert_eq!(far.remaining_quantity(), BaseCurrency::new(6, 0));
    // Fully covered: a third reduce-only sell is rejected.
    assert_eq!(
        ex.submit_limit_order(reduce_only_sell(115, 1)).unwrap_err(),
        SubmitLimitOrderError::ReduceOnlyRejected
    );
    // A plain sell is unaffected by the reduce-only cap.
    let plain = ex
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(108, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap();

    // Market sell 3 -> long 7: keep 4 @ 105 (closest to the touch), cancel 6 @ 110.
    let settlement = ex
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap())
        .unwrap();
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0].id(), far.id());
    let orders = ex.account().active_limit_orders();
    assert!(orders.get_by_id(near.id(), Side::Sell).is_some());
    assert!(orders.get_by_id(plain.id(), Side::Sell).is_some());
    assert_eq!(orders.len(), 2);

    // The rally fills 4 @ 105 and the plain 1 @ 108 -> long 2, never short.
    let events = ex.update_state(&rally_candle()).unwrap();
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(2, 0));
    assert!(ex.account().active_limit_orders().is_empty());
}

#[test]
fn reduce_only_order_is_cancelled_when_a_limit_fill_shrinks_the_position() {
    let mut ex = long_10_exchange();
    let plain = ex
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(105, 0),
                BaseCurrency::new(6, 0),
            )
            .unwrap(),
        )
        .unwrap();
    let ro = ex.submit_limit_order(reduce_only_sell(110, 10)).unwrap();

    // The plain sell fills first (better price), leaving long 4 < 10 -> the reduce-only
    // order is force-cancelled before the rally could fill it.
    let events = ex.update_state(&rally_candle()).unwrap().clone();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(matches!(
        &events[0],
        LimitOrderEvent::Fill(LimitOrderFill::FullyFilled { order_after_fill, .. })
            if order_after_fill.id() == plain.id()
    ));
    assert!(matches!(&events[1], LimitOrderEvent::ForcedCancel(o) if o.id() == ro.id()));
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(4, 0));
}

fn long_10_portfolio()
-> PortfolioExchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(10_000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            mock_exchange_linear().config().contract_spec().clone(),
            OrderRateLimits::default(),
        )],
    )
    .unwrap();
    let mut pf = PortfolioExchange::new(cfg);
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
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    pf
}

#[test]
fn portfolio_resting_reduce_only_order_never_flips_position_after_partial_close() {
    let mut pf = long_10_portfolio();
    let m = MarketId::new(0);
    let ro = pf.submit_limit_order(m, reduce_only_sell(110, 10)).unwrap();

    let settlement = pf
        .submit_market_order(
            m,
            MarketOrder::new(Side::Sell, BaseCurrency::new(6, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(4, 0)
    );
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0], (m, ro.clone()));
    assert!(pf.market(m).unwrap().active_limit_orders.is_empty());

    assert!(pf.update_market(m, &rally_candle()).unwrap().is_empty());
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(4, 0)
    );
}

#[test]
fn portfolio_resting_reduce_only_orders_are_capped_by_position_in_total() {
    let mut pf = long_10_portfolio();
    let m = MarketId::new(0);
    let near = pf.submit_limit_order(m, reduce_only_sell(105, 4)).unwrap();
    let far = pf.submit_limit_order(m, reduce_only_sell(110, 8)).unwrap();
    assert_eq!(far.remaining_quantity(), BaseCurrency::new(6, 0));
    assert!(matches!(
        pf.submit_limit_order(m, reduce_only_sell(115, 1))
            .unwrap_err(),
        PortfolioError::SubmitLimitOrder(SubmitLimitOrderError::ReduceOnlyRejected)
    ));

    let settlement = pf
        .submit_market_order(
            m,
            MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0].1.id(), far.id());
    let slot = pf.market(m).unwrap();
    assert!(
        slot.active_limit_orders
            .get_by_id(near.id(), Side::Sell)
            .is_some()
    );
    assert_eq!(slot.active_limit_orders.len(), 1);

    let events = pf.update_market(m, &rally_candle()).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(3, 0)
    );
}

fn crossing_gtc_buy(
    qty: i64,
) -> LimitOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder> {
    LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(105, 0),
        BaseCurrency::new(qty, 0),
    )
    .unwrap()
    .with_time_in_force(TimeInForce::GoodTilCancelled)
}

fn bba_exchange() -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex
}

#[test]
fn legacy_submit_limit_order_rejects_marketable_orders_without_trading() {
    let mut ex = bba_exchange();
    let balances_before = ex.account().balances().clone();
    let marketable_orders = [
        crossing_gtc_buy(2),
        crossing_gtc_buy(2).with_time_in_force(TimeInForce::ImmediateOrCancel),
        crossing_gtc_buy(2).with_time_in_force(TimeInForce::FillOrKill),
        crossing_gtc_buy(2)
            .with_time_in_force(TimeInForce::GoodTilCrossing)
            .with_re_pricing(RePricing::Marketable),
    ];
    for order in marketable_orders {
        assert_eq!(
            ex.submit_limit_order(order).unwrap_err(),
            SubmitLimitOrderError::WouldExecuteImmediately
        );
    }
    assert_eq!(ex.account().balances(), &balances_before);
    assert!(ex.account().position().quantity().is_zero());
    assert!(ex.account().active_limit_orders().is_empty());

    // Crossing post-only keeps its dedicated rejection, passive orders still rest.
    assert!(matches!(
        ex.submit_limit_order(crossing_gtc_buy(2).with_time_in_force(TimeInForce::GoodTilCrossing)),
        Err(SubmitLimitOrderError::GoodTillCrossingRejectedOrder { .. })
    ));
    let passive = ex
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(99, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap()
            .with_time_in_force(TimeInForce::GoodTilCancelled),
        )
        .unwrap();
    assert_eq!(passive.remaining_quantity(), BaseCurrency::new(2, 0));

    // The execution API fills the same order and reports fee and solvency.
    match ex
        .submit_limit_order_with_execution(crossing_gtc_buy(2))
        .unwrap()
    {
        LimitOrderSubmission::ImmediatelyFilled { fee, solvency, .. } => {
            assert!(fee > QuoteCurrency::zero());
            assert_eq!(solvency, Solvency::Solvent);
        }
        other => panic!("expected an immediate fill, got {other:?}"),
    }
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(2, 0));
}

#[test]
fn amend_to_crossing_price_is_rejected_and_keeps_original_order() {
    let mut ex = bba_exchange();
    let original = ex
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(99, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap()
            .with_time_in_force(TimeInForce::GoodTilCancelled),
        )
        .unwrap();
    let balances_before = ex.account().balances().clone();

    assert_eq!(
        ex.amend_limit_order(original.id(), crossing_gtc_buy(3))
            .unwrap_err(),
        AmendLimitOrderError::SubmitLimitOrder(SubmitLimitOrderError::WouldExecuteImmediately)
    );
    assert!(ex.account().position().quantity().is_zero());
    assert_eq!(ex.account().balances(), &balances_before);
    assert_eq!(
        ex.account()
            .active_limit_orders()
            .get_by_id(original.id(), Side::Buy),
        Some(&original)
    );
}

#[test]
fn failed_amend_restores_original_order() {
    let mut ex = bba_exchange();
    let original = ex
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(99, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        )
        .unwrap();
    // Far more than the 1000 quote wallet can collateralize at 1x leverage.
    let too_large = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(50, 0),
    )
    .unwrap();
    assert!(matches!(
        ex.amend_limit_order(original.id(), too_large).unwrap_err(),
        AmendLimitOrderError::SubmitLimitOrder(SubmitLimitOrderError::NotEnoughAvailableBalance(_))
    ));
    assert_eq!(ex.account().active_limit_orders().len(), 1);
    assert_eq!(
        ex.account()
            .active_limit_orders()
            .get_by_id(original.id(), Side::Buy),
        Some(&original)
    );
}

#[test]
fn portfolio_legacy_limit_apis_reject_marketable_orders_without_trading() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
            mock_exchange_linear().config().contract_spec().clone(),
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
    let balances_before = pf.balances().clone();

    assert!(matches!(
        pf.submit_limit_order(m, crossing_gtc_buy(2)).unwrap_err(),
        PortfolioError::SubmitLimitOrder(SubmitLimitOrderError::WouldExecuteImmediately)
    ));
    assert_eq!(pf.balances(), &balances_before);
    assert!(pf.market(m).unwrap().position.quantity().is_zero());

    let original = pf
        .submit_limit_order(
            m,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(99, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        pf.amend_limit_order(m, original.id(), crossing_gtc_buy(3))
            .unwrap_err(),
        PortfolioError::AmendLimitOrder(AmendLimitOrderError::SubmitLimitOrder(
            SubmitLimitOrderError::WouldExecuteImmediately
        ))
    ));
    let too_large = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(99, 0),
        BaseCurrency::new(50, 0),
    )
    .unwrap();
    assert!(pf.amend_limit_order(m, original.id(), too_large).is_err());
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
    assert_eq!(pf.balances(), &balances_before);
    assert_eq!(
        pf.market(m)
            .unwrap()
            .active_limit_orders
            .get_by_id(original.id(), Side::Buy),
        Some(&original)
    );

    assert!(matches!(
        pf.submit_limit_order_with_execution(m, crossing_gtc_buy(2))
            .unwrap(),
        LimitOrderSubmission::ImmediatelyFilled { .. }
    ));
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(2, 0)
    );
}

fn reduce_only_sell_with_tif(
    qty: i64,
    tif: TimeInForce,
) -> LimitOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder> {
    // Limit 99 crosses the bid of 100.
    reduce_only_sell(99, qty).with_time_in_force(tif)
}

#[test]
fn fill_or_kill_reduce_only_larger_than_position_is_rejected() {
    let mut ex = long_10_exchange();
    let equity = ex.account().balances().equity();
    assert_eq!(
        ex.submit_limit_order_with_execution(reduce_only_sell_with_tif(
            15,
            TimeInForce::FillOrKill
        ))
        .unwrap_err(),
        SubmitLimitOrderError::FillOrKillNotFullyFillable
    );
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(10, 0));
    assert_eq!(ex.account().balances().equity(), equity);

    // IOC may fill partially, so it is clamped to the position instead.
    let sub = ex
        .submit_limit_order_with_execution(reduce_only_sell_with_tif(
            15,
            TimeInForce::ImmediateOrCancel,
        ))
        .unwrap();
    assert!(matches!(
        sub,
        LimitOrderSubmission::ImmediatelyFilled { filled_order, .. }
            if filled_order.total_quantity() == BaseCurrency::new(10, 0)
    ));
    assert!(ex.account().position().quantity().is_zero());
}

#[test]
fn fill_or_kill_reduce_only_within_position_fills() {
    let mut ex = long_10_exchange();
    let sub = ex
        .submit_limit_order_with_execution(reduce_only_sell_with_tif(10, TimeInForce::FillOrKill))
        .unwrap();
    assert!(matches!(
        sub,
        LimitOrderSubmission::ImmediatelyFilled { .. }
    ));
    assert!(ex.account().position().quantity().is_zero());
}

#[test]
fn portfolio_fill_or_kill_reduce_only_larger_than_position_is_rejected() {
    let mut pf = long_10_portfolio();
    let m = MarketId::new(0);
    assert!(matches!(
        pf.submit_limit_order_with_execution(
            m,
            reduce_only_sell_with_tif(15, TimeInForce::FillOrKill)
        )
        .unwrap_err(),
        PortfolioError::SubmitLimitOrder(SubmitLimitOrderError::FillOrKillNotFullyFillable)
    ));
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(10, 0)
    );
    let sub = pf
        .submit_limit_order_with_execution(
            m,
            reduce_only_sell_with_tif(10, TimeInForce::FillOrKill),
        )
        .unwrap();
    assert!(matches!(
        sub,
        LimitOrderSubmission::ImmediatelyFilled { .. }
    ));
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
}

#[test]
fn gtc_then_post_only_re_pricing_is_rejected_when_crossing_on_both_venues() {
    // GTC first, post-only re-pricing last: the order must behave as post-only. Before
    // the fix it kept GTC and traded as a taker although `re_pricing()` said post-only.
    let order = || crossing_gtc_buy(1).with_re_pricing(RePricing::GoodTilCrossing);

    let mut ex = bba_exchange();
    assert!(matches!(
        ex.submit_limit_order_with_execution(order()),
        Err(SubmitLimitOrderError::GoodTillCrossingRejectedOrder { .. })
    ));
    assert!(ex.account().position().quantity().is_zero());

    let mut pf = long_10_portfolio();
    let m = MarketId::new(0);
    let position_before = pf.market(m).unwrap().position.quantity();
    assert!(matches!(
        pf.submit_limit_order_with_execution(m, order()),
        Err(PortfolioError::SubmitLimitOrder(
            SubmitLimitOrderError::GoodTillCrossingRejectedOrder { .. }
        ))
    ));
    assert_eq!(pf.market(m).unwrap().position.quantity(), position_before);
}

#[test]
fn post_only_then_gtc_time_in_force_executes_when_crossing() {
    let order = || {
        crossing_gtc_buy(1)
            .with_re_pricing(RePricing::GoodTilCrossing)
            .with_time_in_force(TimeInForce::GoodTilCancelled)
    };
    let mut ex = bba_exchange();
    assert!(matches!(
        ex.submit_limit_order_with_execution(order()).unwrap(),
        LimitOrderSubmission::ImmediatelyFilled { .. }
    ));
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(1, 0));
}
