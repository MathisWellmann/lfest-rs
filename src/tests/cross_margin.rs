//! Integration tests for `CrossMarginRiskEngine`, live mark-to-market equity,
//! dynamic liquidation pricing, and tiered `MaintenanceMarginSchedule`.

use std::num::NonZeroU16;

use const_decimal::Decimal;

use crate::{
    DECIMALS,
    EXPECT_CONFIG,
    EXPECT_CONTRACT_SPEC,
    EXPECT_DECIMAL,
    EXPECT_NON_ZERO,
    EXPECT_QUANTITY_FILTER,
    prelude::*,
    utils::NoUserOrderId,
};

fn build_cross_linear(
    spec: ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>,
    starting_balance: QuoteCurrency<i64, DECIMALS>,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let config = Config::new(
        starting_balance,
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG)
    .with_margin_mode(MarginMode::Cross);
    Exchange::new(config)
}

#[test]
fn cross_margin_uses_full_wallet_equity_and_survives_beyond_isolated_threshold() {
    // 5x leverage, 10% MM. Isolated would liquidate a long entered at 100 when bid < 90,
    // even if wallet holds 1000 quote backing only 2 units (200 notional)!
    // Cross margin recognizes equity = 1000 covers the 200 notional down to price 0.
    let spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC);

    let mut exchange = build_cross_linear(spec, QuoteCurrency::new(1000, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    // Fully collateralized long: liquidation price is None (<= 0).
    assert_eq!(exchange.estimated_liquidation_price(), None);

    // Price collapses to 50 (would liquidate in isolated mode at 90) -> still well solvent.
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(50, 0),
            ask: QuoteCurrency::new(51, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(2, 0)
    );
    // Mark-to-market equity = 1000 + 2 * (50 - 100) = 900.
    assert_eq!(exchange.mark_to_market_equity(), QuoteCurrency::new(900, 0));
}

#[test]
fn cross_margin_liquidates_at_dynamic_liquidation_price_and_reflects_funding_erosion() {
    // 10x leverage, 5% flat MM (0.05). Wallet = 200.
    // Open long 10 @ 100 -> entry notional = 1000, IM = 100.
    // Dynamic liquidation condition: 200 + 10 * (P - 100) = 10 * P * 0.05
    // => 10 * P * 0.95 = 800 => P = 800 / 9.5 = 84.21052...
    let spec = ContractSpecification::new(
        leverage!(10),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL), // 0.1 * 0.5 = 0.05
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC);

    let mut exchange = build_cross_linear(spec, QuoteCurrency::new(200, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();

    let liq_before = exchange.estimated_liquidation_price().unwrap();
    assert!(liq_before > QuoteCurrency::new(84, 0));
    assert!(liq_before < QuoteCurrency::new(85, 0));

    // Price drops to 85 -> MTM equity = 200 - 150 = 50, MM = 850 * 0.05 = 42.5 -> solvent!
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(85, 0),
            ask: QuoteCurrency::new(86, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert_eq!(exchange.mark_to_market_equity(), QuoteCurrency::new(50, 0));

    // Apply 1% positive funding at mark 85 -> pays 8.5 -> wallet = 191.5, MTM = 41.5 < MM(42.5)!
    // Reconcile immediately liquidates because MTM equity is now below live MM.
    let funding = exchange.apply_funding(
        FundingRate::from(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(85, 0),
    );
    assert_eq!(funding.solvency, Solvency::Liquidated);
    assert!(exchange.account().position().quantity().is_zero());
}

#[test]
fn cross_margin_tiered_brackets_raise_maintenance_margin_for_large_positions() {
    // Tier 1: notional <= 500 -> 5% MM
    // Tier 2: notional > 500  -> 9% MM (continuity offset = 500 * (0.09 - 0.05) = 20)
    // Tier rates must stay below the 10% initial margin rate.
    let schedule = MaintenanceMarginSchedule::from_brackets(&[
        (
            Some(QuoteCurrency::new(500, 0)),
            Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL),
        ),
        (None, Decimal::try_from_scaled(9, 2).expect(EXPECT_DECIMAL)),
    ])
    .unwrap();

    let spec = ContractSpecification::new(
        leverage!(10),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_maintenance_margin_schedule(schedule)
    .expect(EXPECT_CONTRACT_SPEC);

    // Wallet = 150, open long 10 @ 100 (notional = 1000).
    let mut exchange = build_cross_linear(spec, QuoteCurrency::new(150, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();

    // At bid = 92: tier 2 MM = 920 * 0.09 - 20 = 62.8 <= MTM equity 70.
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(92, 0),
            ask: QuoteCurrency::new(93, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    // At bid = 91: MM = 910 * 0.09 - 20 = 61.9 > MTM equity 60 -> LIQUIDATED
    // (flat 5% would only need 45.5)!
    let res = exchange.update_state(&Bba {
        bid: QuoteCurrency::new(91, 0),
        ask: QuoteCurrency::new(92, 0),
        timestamp_exchange_ns: 3.into(),
    });
    assert_eq!(res, Err(RiskError::Liquidate));
}

#[test]
fn cross_margin_unrealized_profit_expands_order_capacity_and_inverse_liq_price_works() {
    // Linear: unrealized profit allows opening additional size.
    let spec = ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC);

    let mut exchange = build_cross_linear(spec, QuoteCurrency::new(100, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    // Buy 2 @ 100 -> uses full 100 initial margin at 2x leverage.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    // Price rises to 150/151 -> uPnL = 2 * (150 - 100) = +100 -> MTM equity = 200 -> 100 available.
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(150, 0),
            ask: QuoteCurrency::new(151, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    // Buy 1 more @ 151 (IM = 75.5 <= 100 available) succeeds in Cross mode!
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(3, 0)
    );

    // Inverse contract liquidation check in Cross mode:
    // Start with 1.0 BTC wallet, 5x leverage, 10% MM.
    let inv_spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL), // mm = 0.10
        PriceFilter::default(),
        QuantityFilter::default(),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC);
    let inv_config = Config::new(
        BaseCurrency::new(1, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        inv_spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG)
    .with_margin_mode(MarginMode::Cross);
    let mut inv =
        Exchange::<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId>::new(inv_config);
    inv.update_state(&Bba {
        bid: QuoteCurrency::new(99, 0),
        ask: QuoteCurrency::new(100, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    // Long 300 USD @ 100 -> 3 BTC notional (3x effective leverage).
    // Liq condition: 1 + 300 * (1/100 - 1/P) = (300/P) * 0.10 => 4 = 330/P => P = 82.5.
    inv.submit_market_order(MarketOrder::new(Side::Buy, QuoteCurrency::new(300, 0)).unwrap())
        .unwrap();
    let inv_liq = inv.estimated_liquidation_price().unwrap();
    assert_eq!(inv_liq, QuoteCurrency::new(825, 1));

    // At bid = 83 -> solvent.
    inv.update_state(&Bba {
        bid: QuoteCurrency::new(83, 0),
        ask: QuoteCurrency::new(84, 0),
        timestamp_exchange_ns: 2.into(),
    })
    .unwrap();
    // At bid = 82 (< 82.5) -> liquidated.
    assert_eq!(
        inv.update_state(&Bba {
            bid: QuoteCurrency::new(82, 0),
            ask: QuoteCurrency::new(83, 0),
            timestamp_exchange_ns: 3.into(),
        }),
        Err(RiskError::Liquidate)
    );
}

fn two_tier_spec_2x() -> ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    // Tier 1: notional <= 500 -> 5%, tier 2: 20% with continuity offset 75.
    let schedule = MaintenanceMarginSchedule::from_brackets(&[
        (
            Some(QuoteCurrency::new(500, 0)),
            Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL),
        ),
        (None, Decimal::try_from_scaled(2, 1).expect(EXPECT_DECIMAL)),
    ])
    .unwrap();
    ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_maintenance_margin_schedule(schedule)
    .expect(EXPECT_CONTRACT_SPEC)
}

#[test]
fn isolated_margin_uses_tiered_maintenance_schedule() {
    // Long 10 @ 100 (notional 1000, tier 2): MM = 1000 * 0.2 - 75 = 125 -> effective rate
    // 12.5%. With 50% initial margin the position survives a move of 50% - 12.5% = 37.5%,
    // so it liquidates below 62.5. The tier-0 rate (5%) alone would allow a 45% move (55).
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        two_tier_spec_2x(),
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    assert_eq!(config.margin_mode(), MarginMode::Isolated);
    let mut exchange =
        Exchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(config);
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.estimated_liquidation_price(),
        Some(QuoteCurrency::new(625, 1))
    );
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(63, 0),
            ask: QuoteCurrency::new(64, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    // Between the tiered (62.5) and the tier-0 (55) threshold: only the schedule liquidates.
    assert_eq!(
        exchange.update_state(&Bba {
            bid: QuoteCurrency::new(62, 0),
            ask: QuoteCurrency::new(63, 0),
            timestamp_exchange_ns: 3.into(),
        }),
        Err(RiskError::Liquidate)
    );
}

#[test]
fn isolated_short_uses_tiered_maintenance_schedule() {
    // Short 10 @ 100 (notional 1000, tier 2, effective MM rate 12.5%) liquidates above 137.5.
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        two_tier_spec_2x(),
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    let mut exchange =
        Exchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(config);
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.estimated_liquidation_price(),
        Some(QuoteCurrency::new(1375, 1))
    );
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(136, 0),
            ask: QuoteCurrency::new(137, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert_eq!(
        exchange.update_state(&Bba {
            bid: QuoteCurrency::new(137, 0),
            ask: QuoteCurrency::new(138, 0),
            timestamp_exchange_ns: 3.into(),
        }),
        Err(RiskError::Liquidate)
    );
}

#[test]
fn maintenance_schedule_must_stay_below_initial_margin_rate() {
    let spec = || {
        ContractSpecification::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>::new(
            leverage!(5), // imr = 20%
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
            PriceFilter::default(),
            QuantityFilter::default(),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::zero()),
        )
        .expect(EXPECT_CONTRACT_SPEC)
    };
    let schedule = |top_rate| {
        MaintenanceMarginSchedule::from_brackets(&[
            (
                Some(QuoteCurrency::new(500, 0)),
                Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL),
            ),
            (None, top_rate),
        ])
        .unwrap()
    };
    // Top tier at exactly the initial margin rate: liquidatable at open.
    assert!(matches!(
        spec().with_maintenance_margin_schedule(schedule(
            Decimal::try_from_scaled(2, 1).expect(EXPECT_DECIMAL)
        )),
        Err(ConfigError::InvalidMaintenanceMarginTier)
    ));
    // Just below the initial margin rate is valid.
    let ok = spec()
        .with_maintenance_margin_schedule(schedule(
            Decimal::try_from_scaled(19, 2).expect(EXPECT_DECIMAL),
        ))
        .expect(EXPECT_CONTRACT_SPEC);
    assert_eq!(
        ok.maintenance_margin(),
        Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL)
    );
}
