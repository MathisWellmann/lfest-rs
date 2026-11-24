//! Tests for `SmartCandle` volume depletion, `VolumeCandle` participation caps,
//! `SlippageModel` market impact on `Exchange` & `PortfolioExchange`, and `Ledger` reconciliation.

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

#[test]
fn smart_candle_depletes_volume_across_multiple_resting_orders() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();

    // Rest two buy limit orders: qty 3 @ 100 and qty 3 @ 99.
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(3, 0),
        )
        .unwrap(),
    )
    .unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(3, 0),
        )
        .unwrap(),
    )
    .unwrap();

    // SmartCandle has total sell taker volume = 4 (qty 2 @ 98 and qty 2 @ 99).
    // Before the fix, both orders would have filled completely (6 total > 4 available!).
    // With volume depletion:
    // - Order @ 100 fills 3 (consumes 3 of the 4 sell units, leaving 1).
    // - Order @ 99 only sees sell volume strictly below 99 (originally 2 @ 98, clamped to remaining 1).
    // Total filled must be exactly 4 (never exceeding total bar sell volume!).
    let trades = &[
        Trade {
            timestamp_exchange_ns: 2.into(),
            price: QuoteCurrency::new(98, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Sell,
        },
        Trade {
            timestamp_exchange_ns: 3.into(),
            price: QuoteCurrency::new(99, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Sell,
        },
    ];
    let bba = Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 3.into(),
    };
    let sc = SmartCandle::new(trades, bba, ex.config().contract_spec().price_filter());

    let events = ex.update_state(&sc).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(4, 0));
    // Second order should still have 2 remaining resting on the book.
    assert_eq!(ex.account().active_limit_orders().len(), 1);
    assert_eq!(
        ex.account()
            .active_limit_orders()
            .best_bid()
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(2, 0)
    );
}

#[test]
fn volume_candle_enforces_participation_cap_across_resting_orders() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();

    // Rest two buy orders totaling qty 6.
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(4, 0),
        )
        .unwrap(),
    )
    .unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(98, 0),
            BaseCurrency::new(2, 0),
        )
        .unwrap(),
    )
    .unwrap();

    // Bar volume = 50, participation_rate = 10% -> max fillable bid qty = 5.
    let vc = VolumeCandle::new(
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(101, 0),
        QuoteCurrency::new(95, 0),
        QuoteCurrency::new(102, 0),
        BaseCurrency::new(50, 0),
        Decimal::try_from_scaled(1, 1).expect(EXPECT_DECIMAL),
        2.into(),
    )
    .unwrap();

    let events = ex.update_state(&vc).unwrap();
    assert_eq!(events.len(), 2);
    // Order @ 99 fills 4, order @ 98 partially fills 1 -> total position = 5 (capped at 10% of 50).
    assert_eq!(ex.account().position().quantity(), BaseCurrency::new(5, 0));
    assert_eq!(
        ex.account()
            .active_limit_orders()
            .best_bid()
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(1, 0)
    );
}

#[test]
fn slippage_model_and_ledger_reconcile_exact_equity_on_exchange_and_portfolio() {
    // 1% fixed slippage, 0.1% taker fee.
    let slip =
        SlippageModel::fixed_rate(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL)).unwrap();
    let spec = ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL)),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_slippage_model(slip)
    .with_ticker("BTCUSDT");

    let cfg = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        spec.clone(),
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    let mut ex = Exchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    ex.enable_ledger(32);

    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(100, 0) + QuoteCurrency::new(1, 0), // 101
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();

    // Buy 2 @ ask 101 + 1% slippage = 102.01.
    let open = ex
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    assert_eq!(
        open.filled_order.state().avg_fill_price(),
        QuoteCurrency::new(10201, 2)
    );

    // Apply funding (+0.5% at mark 100 -> pays 1.0).
    let _ = ex.apply_funding(
        FundingRate::from(Decimal::try_from_scaled(5, 3).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(100, 0),
    );

    // Close 2 @ bid 100 - 1% slippage = 99.00.
    let close = ex
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    assert_eq!(
        close.filled_order.state().avg_fill_price(),
        QuoteCurrency::new(99, 0)
    );

    let ledger = ex.ledger().unwrap();
    assert_eq!(ledger.len(), 3);
    assert!(ledger.reconciles_to(ex.account().balances().equity()));

    // Same accounting reconciliation on PortfolioExchange.
    let pf_cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            spec,
            OrderRateLimits::default(),
        )],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(pf_cfg);
    pf.enable_ledger(32);
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
    pf.submit_market_order(
        btc,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    pf.apply_funding(
        btc,
        FundingRate::from(Decimal::try_from_scaled(5, 3).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(100, 0),
    )
    .unwrap();
    pf.delist_market(btc, QuoteCurrency::new(110, 0)).unwrap();

    let pf_ledger = pf.ledger().unwrap();
    assert_eq!(pf_ledger.len(), 3);
    assert!(pf_ledger.reconciles_to(pf.balances().equity()));
}

/// Sell flow of exactly 2 below the resting bid (exhausting the bid side) and buy flow
/// of 2 above the resting ask.
fn two_sided_smart_candle(
    price_filter: &PriceFilter<i64, DECIMALS>,
) -> SmartCandle<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    let trades = &[
        Trade {
            timestamp_exchange_ns: 2.into(),
            price: QuoteCurrency::new(98, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Sell,
        },
        Trade {
            timestamp_exchange_ns: 3.into(),
            price: QuoteCurrency::new(103, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Buy,
        },
    ];
    let bba = Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 3.into(),
    };
    SmartCandle::new(trades, bba, price_filter)
}

fn resting_bid_and_ask()
-> [LimitOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder>; 2] {
    [
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(2, 0),
        )
        .unwrap(),
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(102, 0),
            BaseCurrency::new(2, 0),
        )
        .unwrap(),
    ]
}

#[test]
fn smart_candle_bid_exhaustion_still_fills_resting_asks() {
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    for order in resting_bid_and_ask() {
        ex.submit_limit_order(order).unwrap();
    }

    let sc = two_sided_smart_candle(ex.config().contract_spec().price_filter());
    let events = ex.update_state(&sc).unwrap();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(
        events
            .iter()
            .all(|e| matches!(e, LimitOrderEvent::Fill(LimitOrderFill::FullyFilled { .. })))
    );
    assert!(ex.account().active_limit_orders().is_empty());
    assert!(ex.account().position().quantity().is_zero());
}

#[test]
fn portfolio_smart_candle_bid_exhaustion_still_fills_resting_asks() {
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
    for order in resting_bid_and_ask() {
        pf.submit_limit_order(m, order).unwrap();
    }

    let sc = two_sided_smart_candle(&PriceFilter::default());
    let events = pf.update_market(m, &sc).unwrap();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(pf.market(m).unwrap().active_limit_orders.is_empty());
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
}
