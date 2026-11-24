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

/// 1% fixed slippage, 0.1% taker fee, tick 0.01.
fn slippage_ledger_spec() -> ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    let slip =
        SlippageModel::fixed_rate(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL)).unwrap();
    // A tick of 0.01, so the 1% impact is not snapped to whole prices.
    ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 2),
            Decimal::TWO,
            Decimal::zero(),
        )
        .expect("valid price filter"),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL)),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_slippage_model(slip)
    .with_ticker("BTCUSDT")
}

#[test]
fn slippage_model_and_ledger_reconcile_exact_equity_on_exchange() {
    let spec = slippage_ledger_spec();
    let cfg = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        spec,
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

    // Open: no pnl, fee 204.02 * 0.1%. Funding: pays 2 * 100 * 0.5%.
    // Close: pnl (99 - 102.01) * 2, fee 198 * 0.1%.
    let ledger = ex.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, QuoteCurrency::new(-20402, 5)),
            (LedgerEntryKind::Funding, QuoteCurrency::new(-1, 0)),
            (LedgerEntryKind::RealizedPnl, QuoteCurrency::new(-602, 2)),
            (LedgerEntryKind::TradingFee, QuoteCurrency::new(-198, 3)),
        ]
    );
    assert_eq!(
        ex.account().balances().equity(),
        QuoteCurrency::new(99257798, 5)
    );
    assert!(ledger.reconciles_to(ex.account().balances().equity()));
    assert_eq!(
        -ledger.total_by_kind(LedgerEntryKind::TradingFee),
        ex.account().balances().total_fees_paid()
    );
}

#[test]
fn slippage_model_and_ledger_reconcile_exact_equity_on_portfolio() {
    let pf_cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            slippage_ledger_spec(),
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

    // Delisting at 110 settles pnl (110 - 102.01) * 2 without a fee.
    let pf_ledger = pf.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(pf_ledger),
        vec![
            (LedgerEntryKind::TradingFee, QuoteCurrency::new(-20402, 5)),
            (LedgerEntryKind::Funding, QuoteCurrency::new(-1, 0)),
            (
                LedgerEntryKind::DelistSettlement,
                QuoteCurrency::new(1598, 2)
            ),
        ]
    );
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(101477598, 5));
    assert!(pf_ledger.reconciles_to(pf.balances().equity()));
}

fn kinds_and_deltas(
    ledger: &Ledger<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>,
) -> Vec<(LedgerEntryKind, QuoteCurrency<i64, DECIMALS>)> {
    ledger
        .entries()
        .iter()
        .map(|e| (e.kind(), e.delta()))
        .collect()
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

/// Linear contract: leverage 5, maker fee 0.02%, taker fee 0.1%, liquidation fee 5%.
fn ledger_spec() -> ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::try_from_scaled(2, 4).expect(EXPECT_DECIMAL)),
        Fee::from(Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL)),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_liquidation_fee(Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL))
    .unwrap()
    .with_ticker("BTCUSDT")
}

fn ledger_exchange(
    balance: i64,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let cfg = Config::new(
        QuoteCurrency::new(balance, 0),
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        ledger_spec(),
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    let mut ex = Exchange::new(cfg);
    ex.enable_ledger(32);
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(99, 0),
        ask: QuoteCurrency::new(100, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex
}

fn ledger_portfolio(
    balance: i64,
) -> PortfolioExchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(balance, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            ledger_spec(),
            OrderRateLimits::default(),
        )],
    )
    .unwrap();
    let mut pf = PortfolioExchange::new(cfg);
    pf.enable_ledger(32);
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

fn q(integer: i64, scale: u8) -> QuoteCurrency<i64, DECIMALS> {
    QuoteCurrency::new(integer, scale)
}

#[test]
fn ledger_records_maker_fill_pnl_and_fee_separately() {
    // Long 10 @ 100 (taker fee 1), then a resting sell 10 @ 110 fills as maker:
    // pnl +100, maker fee 1100 * 0.02% = 0.22.
    let mut ex = ledger_exchange(1000);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    let sell = LimitOrder::new(Side::Sell, q(110, 0), BaseCurrency::new(10, 0)).unwrap();
    ex.submit_limit_order(sell).unwrap();
    ex.update_state(&Trade {
        timestamp_exchange_ns: 2.into(),
        price: q(111, 0),
        quantity: BaseCurrency::new(10, 0),
        side: Side::Buy,
    })
    .unwrap();
    assert!(ex.account().position().quantity().is_zero());

    let ledger = ex.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, q(-1, 0)),
            (LedgerEntryKind::RealizedPnl, q(100, 0)),
            (LedgerEntryKind::TradingFee, q(-22, 2)),
        ]
    );
    assert_eq!(ex.account().balances().equity(), q(109878, 2));
    assert!(ledger.reconciles_to(ex.account().balances().equity()));
    assert_eq!(
        ledger.entries().last().unwrap().equity_after(),
        q(109878, 2)
    );
}

#[test]
fn portfolio_ledger_records_maker_fill_pnl_and_fee_separately() {
    let mut pf = ledger_portfolio(1000);
    let m = MarketId::new(0);
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    let sell = LimitOrder::new(Side::Sell, q(110, 0), BaseCurrency::new(10, 0)).unwrap();
    pf.submit_limit_order(m, sell).unwrap();
    pf.update_market(
        m,
        &Trade {
            timestamp_exchange_ns: 2.into(),
            price: q(111, 0),
            quantity: BaseCurrency::new(10, 0),
            side: Side::Buy,
        },
    )
    .unwrap();
    assert!(pf.market(m).unwrap().position.quantity().is_zero());

    let ledger = pf.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, q(-1, 0)),
            (LedgerEntryKind::RealizedPnl, q(100, 0)),
            (LedgerEntryKind::TradingFee, q(-22, 2)),
        ]
    );
    assert!(ledger.reconciles_to(pf.balances().equity()));
    assert!(ledger.entries().iter().all(|e| e.market_id() == Some(m)));
}

#[test]
fn ledger_records_nominal_bankrupting_liquidation_and_bad_debt_credit() {
    // Long 10 @ 100 from 210 (fee 1 -> 209). Gap to bid 70: pnl -300, taker fee 0.7,
    // clearance fee waived, venue absorbs 91.7.
    let mut ex = ledger_exchange(210);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    let res = ex.update_state(&Bba {
        bid: q(70, 0),
        ask: q(71, 0),
        timestamp_exchange_ns: 2.into(),
    });
    assert_eq!(res, Err(RiskError::Liquidate));
    assert_eq!(ex.account().balances().bad_debt(), q(917, 1));

    let ledger = ex.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, q(-1, 0)),
            (LedgerEntryKind::Liquidation, q(-300, 0)),
            (LedgerEntryKind::TradingFee, q(-7, 1)),
            (LedgerEntryKind::BadDebtAbsorption, q(917, 1)),
        ]
    );
    // The running equity dips below zero until the venue's credit.
    assert_eq!(ledger.entries()[2].equity_after(), q(-917, 1));
    assert!(ledger.entries()[3].equity_after().is_zero());
    assert!(ledger.reconciles_to(ex.account().balances().equity()));
    assert_eq!(
        ledger.total_by_kind(LedgerEntryKind::BadDebtAbsorption),
        ex.account().balances().bad_debt()
    );
}

#[test]
fn portfolio_ledger_records_nominal_bankrupting_liquidation_and_bad_debt_credit() {
    let mut pf = ledger_portfolio(210);
    let m = MarketId::new(0);
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    let res = pf.update_market(
        m,
        &Bba {
            bid: q(70, 0),
            ask: q(71, 0),
            timestamp_exchange_ns: 2.into(),
        },
    );
    assert!(matches!(
        res,
        Err(PortfolioError::Risk(RiskError::Liquidate))
    ));
    assert_eq!(pf.balances().bad_debt(), q(917, 1));
    let ledger = pf.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, q(-1, 0)),
            (LedgerEntryKind::Liquidation, q(-300, 0)),
            (LedgerEntryKind::TradingFee, q(-7, 1)),
            (LedgerEntryKind::BadDebtAbsorption, q(917, 1)),
        ]
    );
    assert!(ledger.reconciles_to(pf.balances().equity()));
}

#[test]
fn ledger_records_capped_clearance_fee_as_liquidation_cost() {
    // Gap to bid 80: pnl -200, taker fee 0.8, the clearance fee is capped at the 8.2 left.
    let mut ex = ledger_exchange(210);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    let _ = ex.update_state(&Bba {
        bid: q(80, 0),
        ask: q(81, 0),
        timestamp_exchange_ns: 2.into(),
    });
    let ledger = ex.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, q(-1, 0)),
            (LedgerEntryKind::Liquidation, q(-200, 0)),
            (LedgerEntryKind::TradingFee, q(-8, 1)),
            (LedgerEntryKind::Liquidation, q(-82, 1)),
        ]
    );
    assert!(ex.account().balances().equity().is_zero());
    assert!(ledger.reconciles_to(ex.account().balances().equity()));
}

#[test]
fn ledger_records_funding_and_marketable_limit_and_stop_fills() {
    // Long 10 @ 100 (fee 1). A rate of -1% at mark 100 pays the long 10.
    let mut ex = ledger_exchange(1000);
    ex.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    let settlement = ex.apply_funding(
        FundingRate::from(Decimal::try_from_scaled(-1, 2).expect(EXPECT_DECIMAL)),
        q(100, 0),
    );
    assert_eq!(settlement.cashflow, q(10, 0));
    // Marketable sell limit 4 @ 95 fills at the bid 99: pnl -4, fee 396 * 0.1%.
    let sell = LimitOrder::new(Side::Sell, q(95, 0), BaseCurrency::new(4, 0))
        .unwrap()
        .with_time_in_force(TimeInForce::GoodTilCancelled);
    ex.submit_limit_order_with_execution(sell).unwrap();
    // Sell stop 6 @ 90, elected when the bid falls to 90: pnl -60, fee 540 * 0.1%.
    ex.submit_stop_order(
        StopOrder::stop_market(Side::Sell, q(90, 0), BaseCurrency::new(6, 0), NoUserOrderId)
            .unwrap()
            .with_trigger_price_type(TriggerPriceType::BestBidAsk),
    )
    .unwrap();
    ex.update_state(&Bba {
        bid: q(90, 0),
        ask: q(91, 0),
        timestamp_exchange_ns: 2.into(),
    })
    .unwrap();
    assert!(ex.account().position().quantity().is_zero());

    let ledger = ex.ledger().unwrap();
    assert_eq!(
        kinds_and_deltas(ledger),
        vec![
            (LedgerEntryKind::TradingFee, q(-1, 0)),
            (LedgerEntryKind::Funding, q(10, 0)),
            (LedgerEntryKind::RealizedPnl, q(-4, 0)),
            (LedgerEntryKind::TradingFee, q(-396, 3)),
            (LedgerEntryKind::RealizedPnl, q(-60, 0)),
            (LedgerEntryKind::TradingFee, q(-54, 2)),
        ]
    );
    assert!(ledger.reconciles_to(ex.account().balances().equity()));
    assert_eq!(
        ledger.total_by_kind(LedgerEntryKind::Funding),
        ex.account().balances().cumulative_funding()
    );
    assert_eq!(
        -ledger.total_by_kind(LedgerEntryKind::TradingFee),
        ex.account().balances().total_fees_paid()
    );
}
