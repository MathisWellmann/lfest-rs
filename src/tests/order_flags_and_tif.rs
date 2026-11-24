//! Tests for `TimeInForce` (`GTX`, `GTC`, `IOC`, `FOK`), `RePricing::Marketable`,
//! and `reduce_only` orders on both `Exchange` and `PortfolioExchange`.

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
