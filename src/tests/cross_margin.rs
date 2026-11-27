//! Integration tests for `CrossMarginRiskEngine`, live mark-to-market equity,
//! dynamic liquidation pricing, and tiered `MaintenanceMarginSchedule`.

use std::num::NonZeroU16;

use const_decimal::Decimal;

use super::bba;
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

type LinearExchange = Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>;
type LinearSpec = ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>;

/// Linear contract with a maintenance fraction of 0.5, a quantity step of 0.01 and no
/// maker fee, so the expected margins in the tests stay easy to compute by hand.
fn linear_spec(leverage: Leverage<i64, DECIMALS>, fee_taker: Decimal<i64, DECIMALS>) -> LinearSpec {
    ContractSpecification::new(
        leverage,
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(fee_taker),
    )
    .expect(EXPECT_CONTRACT_SPEC)
}

/// [`linear_spec`] without any fees.
fn fee_free_linear_spec(leverage: Leverage<i64, DECIMALS>) -> LinearSpec {
    linear_spec(leverage, Decimal::zero())
}

/// A linear config in the default (isolated) margin mode.
fn linear_config(
    spec: LinearSpec,
    starting_balance: QuoteCurrency<i64, DECIMALS>,
) -> Config<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>> {
    Config::new(
        starting_balance,
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG)
}

fn build_cross_linear(
    spec: LinearSpec,
    starting_balance: QuoteCurrency<i64, DECIMALS>,
) -> LinearExchange {
    Exchange::new(linear_config(spec, starting_balance).with_margin_mode(MarginMode::Cross))
}

#[test]
fn cross_margin_uses_full_wallet_equity_and_survives_beyond_isolated_threshold() {
    // 5x leverage, 10% MM. Isolated would liquidate a long entered at 100 when bid < 90,
    // even if wallet holds 1000 quote backing only 2 units (200 notional)!
    // Cross margin recognizes equity = 1000 covers the 200 notional down to price 0.
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(5)),
        QuoteCurrency::new(1000, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    // Fully collateralized long: liquidation price is None (<= 0).
    assert_eq!(exchange.estimated_liquidation_price(), None);

    // Price collapses to 50 (would liquidate in isolated mode at 90) -> still well solvent.
    exchange.update_state(&bba(50, 51, 2)).unwrap();
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(2, 0)
    );
    // Mark-to-market equity = 1000 + 2 * (50 - 100) = 900.
    assert_eq!(exchange.mark_to_market_equity(), QuoteCurrency::new(900, 0));
}

#[test]
fn cross_margin_liquidates_at_dynamic_liquidation_price_and_reflects_funding_erosion() {
    // 10x leverage, 5% flat MM (0.1 * 0.5 = 0.05). Wallet = 200.
    // Open long 10 @ 100 -> entry notional = 1000, IM = 100.
    // Dynamic liquidation condition: 200 + 10 * (P - 100) = 10 * P * 0.05
    // => 10 * P * 0.95 = 800 => P = 800 / 9.5 = 84.21052...
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(10)),
        QuoteCurrency::new(200, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();

    let liq_before = exchange.estimated_liquidation_price().unwrap();
    assert!(liq_before > QuoteCurrency::new(84, 0));
    assert!(liq_before < QuoteCurrency::new(85, 0));

    // Price drops to 85 -> MTM equity = 200 - 150 = 50, MM = 850 * 0.05 = 42.5 -> solvent!
    exchange.update_state(&bba(85, 86, 2)).unwrap();
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
    let spec = fee_free_linear_spec(leverage!(10))
        .with_maintenance_margin_schedule(schedule)
        .expect(EXPECT_CONTRACT_SPEC);

    // Wallet = 150, open long 10 @ 100 (notional = 1000).
    let mut exchange = build_cross_linear(spec, QuoteCurrency::new(150, 0));
    exchange.update_state(&bba(99, 100, 1)).unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();

    // At bid = 92: tier 2 MM = 920 * 0.09 - 20 = 62.8 <= MTM equity 70.
    exchange.update_state(&bba(92, 93, 2)).unwrap();
    // At bid = 91: MM = 910 * 0.09 - 20 = 61.9 > MTM equity 60 -> LIQUIDATED
    // (flat 5% would only need 45.5)!
    assert_eq!(
        exchange.update_state(&bba(91, 92, 3)),
        Err(RiskError::Liquidate)
    );
}

#[test]
fn cross_margin_unrealized_profit_expands_order_capacity_and_inverse_liq_price_works() {
    // Linear: unrealized profit allows opening additional size.
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(2)),
        QuoteCurrency::new(100, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    // Buy 2 @ 100 -> uses full 100 initial margin at 2x leverage.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    // Price rises to 150/151 -> uPnL = 2 * (150 - 100) = +100 -> MTM equity = 200 -> 100 available.
    exchange.update_state(&bba(150, 151, 2)).unwrap();
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
    inv.update_state(&bba(99, 100, 1)).unwrap();
    // Long 300 USD @ 100 -> 3 BTC notional (3x effective leverage).
    // Liq condition: 1 + 300 * (1/100 - 1/P) = (300/P) * 0.10 => 4 = 330/P => P = 82.5.
    inv.submit_market_order(MarketOrder::new(Side::Buy, QuoteCurrency::new(300, 0)).unwrap())
        .unwrap();
    let inv_liq = inv.estimated_liquidation_price().unwrap();
    assert_eq!(inv_liq, QuoteCurrency::new(825, 1));

    // At bid = 83 -> solvent.
    inv.update_state(&bba(83, 84, 2)).unwrap();
    // At bid = 82 (< 82.5) -> liquidated.
    assert_eq!(inv.update_state(&bba(82, 83, 3)), Err(RiskError::Liquidate));
}

#[test]
fn cross_margin_limit_order_admission_counts_unrealized_pnl_up_to_zero_excess() {
    // 2x leverage, wallet 100: long 2 @ 100 locks the whole wallet as position margin.
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(2)),
        QuoteCurrency::new(100, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    // uPnL = 2 * (150 - 100) = +100 is the only collateral left for new orders.
    exchange.update_state(&bba(150, 151, 2)).unwrap();

    // Excess = 100 - 100 (position) - 70 (order) + 100 (uPnL) = 30: rests, but only
    // because the unrealized profit is added.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(140, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap();
    // Excess = 30 - 30 = 0 exactly: an order that uses all collateral still rests.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(6, 1),
            )
            .unwrap(),
        )
        .unwrap();
    // One more step of size makes the excess negative.
    assert_eq!(
        exchange.submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(1, 2)
            )
            .unwrap(),
        ),
        Err(NotEnoughAvailableBalance.into())
    );
    assert_eq!(exchange.account().active_limit_orders().num_active(), 2);
}

#[test]
fn cross_margin_limit_order_is_rejected_when_unrealized_loss_eats_the_collateral() {
    // Wallet 100 at 2x: long 1 @ 100 locks 50, the bid of 80 marks a uPnL of -20.
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(2)),
        QuoteCurrency::new(100, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    exchange.update_state(&bba(80, 81, 2)).unwrap();

    // Wallet-only excess = 100 - 50 - 40 = 10, but with the uPnL it is -10.
    assert_eq!(
        exchange.submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(80, 0),
                BaseCurrency::new(1, 0)
            )
            .unwrap(),
        ),
        Err(NotEnoughAvailableBalance.into())
    );
    assert_eq!(exchange.account().active_limit_orders().num_active(), 0);
}

/// 2x leverage and a taker fee of 0.1%, so fees decide the admission at the boundary.
fn cross_linear_with_taker_fee(wallet: QuoteCurrency<i64, DECIMALS>) -> LinearExchange {
    let fee_taker = Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL);
    build_cross_linear(linear_spec(leverage!(2), fee_taker), wallet)
}

#[test]
fn cross_margin_market_order_initial_margin_and_fee_must_fit_available_margin() {
    // Buy 1 @ 100 needs IM 50 plus a fee of 0.1 = 50.1, the wallet is 0.01 short.
    let mut exchange = cross_linear_with_taker_fee(QuoteCurrency::new(5009, 2));
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    assert_eq!(
        exchange.submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap()),
        Err(NotEnoughAvailableBalance.into())
    );

    let mut exchange = cross_linear_with_taker_fee(QuoteCurrency::new(501, 1));
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(1, 0)
    );
}

#[test]
fn cross_margin_market_sell_increasing_a_short_needs_available_margin() {
    // Short 2 @ 100 needs IM 100 plus a fee of 0.2, which uses the whole wallet.
    let mut exchange = cross_linear_with_taker_fee(QuoteCurrency::new(1002, 1));
    exchange.update_state(&bba(100, 101, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    // Selling more increases the short, so it is not a reducing order even though its
    // quantity is below the position size.
    assert_eq!(
        exchange
            .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap()),
        Err(NotEnoughAvailableBalance.into())
    );
    assert_eq!(
        exchange.account().position().quantity(),
        -BaseCurrency::new(2, 0)
    );
}

/// Long 1 @ 100 with the bid at 99, then a market sell of 3 flips it to a short of 2.
/// The flip closes 1 for a realized pnl of -1 and opens a short of 2 @ 99 with IM 99.
/// The fees are 0.1 for the entry and 0.297 for the flip, so a wallet of 100.397 leaves
/// a post-flip equity of exactly 99.
fn submit_flip(
    wallet: QuoteCurrency<i64, DECIMALS>,
) -> (LinearExchange, Result<(), SubmitMarketOrderError>) {
    let mut exchange = cross_linear_with_taker_fee(wallet);
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    let res = exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap())
        .map(|_| ());
    (exchange, res)
}

#[test]
fn cross_margin_market_order_flip_with_exactly_enough_post_flip_equity_is_accepted() {
    let (exchange, res) = submit_flip(QuoteCurrency::new(100_397, 3));
    assert_eq!(res, Ok(()));
    assert_eq!(
        exchange.account().position().quantity(),
        -BaseCurrency::new(2, 0)
    );
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(99, 0)
    );
}

#[test]
fn cross_margin_market_order_flip_short_of_post_flip_margin_is_rejected() {
    // 0.001 less wallet leaves a post-flip equity of 98.999 < 99.
    let (exchange, res) = submit_flip(QuoteCurrency::new(100_396, 3));
    assert_eq!(res, Err(NotEnoughAvailableBalance.into()));
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(1, 0)
    );
}

#[test]
fn cross_margin_market_order_reducing_a_position_is_always_accepted() {
    // The same wallet that cannot afford the flip can always close the long.
    let mut exchange = cross_linear_with_taker_fee(QuoteCurrency::new(100_396, 3));
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    assert!(exchange.account().position().quantity().is_zero());
}

fn two_tier_spec_2x() -> LinearSpec {
    // Tier 1: notional <= 500 -> 5%, tier 2: 20% with continuity offset 75.
    let schedule = MaintenanceMarginSchedule::from_brackets(&[
        (
            Some(QuoteCurrency::new(500, 0)),
            Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL),
        ),
        (None, Decimal::try_from_scaled(2, 1).expect(EXPECT_DECIMAL)),
    ])
    .unwrap();
    fee_free_linear_spec(leverage!(2))
        .with_maintenance_margin_schedule(schedule)
        .expect(EXPECT_CONTRACT_SPEC)
}

#[test]
fn isolated_margin_uses_tiered_maintenance_schedule() {
    // Long 10 @ 100 (notional 1000, tier 2): MM = 1000 * 0.2 - 75 = 125 -> effective rate
    // 12.5%. With 50% initial margin the position survives a move of 50% - 12.5% = 37.5%,
    // so it liquidates below 62.5. The tier-0 rate (5%) alone would allow a 45% move (55).
    let config = linear_config(two_tier_spec_2x(), QuoteCurrency::new(1000, 0));
    assert_eq!(config.margin_mode(), MarginMode::Isolated);
    let mut exchange = LinearExchange::new(config);
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.estimated_liquidation_price(),
        Some(QuoteCurrency::new(625, 1))
    );
    exchange.update_state(&bba(63, 64, 2)).unwrap();
    // Between the tiered (62.5) and the tier-0 (55) threshold: only the schedule liquidates.
    assert_eq!(
        exchange.update_state(&bba(62, 63, 3)),
        Err(RiskError::Liquidate)
    );
}

#[test]
fn isolated_short_uses_tiered_maintenance_schedule() {
    // Short 10 @ 100 (notional 1000, tier 2, effective MM rate 12.5%) liquidates above 137.5.
    let mut exchange = LinearExchange::new(linear_config(
        two_tier_spec_2x(),
        QuoteCurrency::new(1000, 0),
    ));
    exchange.update_state(&bba(100, 101, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.estimated_liquidation_price(),
        Some(QuoteCurrency::new(1375, 1))
    );
    exchange.update_state(&bba(136, 137, 2)).unwrap();
    assert_eq!(
        exchange.update_state(&bba(137, 138, 3)),
        Err(RiskError::Liquidate)
    );
}

#[test]
fn maintenance_schedule_must_stay_below_initial_margin_rate() {
    // imr = 20%
    let spec = || fee_free_linear_spec(leverage!(5));
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

/// Settle a funding rate of zero, which only runs the margin reconciliation.
fn reconcile(
    exchange: &mut LinearExchange,
) -> FundingSettlement<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    exchange.apply_funding(
        FundingRate::from(Decimal::zero()),
        QuoteCurrency::new(100, 0),
    )
}

#[test]
fn cross_margin_call_cancels_resting_orders_once_unrealized_loss_eats_the_excess() {
    // Wallet 100 at 2x: long 1 @ 100 (IM 50) and a resting buy 1 @ 80 (IM 40).
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(2)),
        QuoteCurrency::new(100, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(80, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap();
    // The wallet-only excess stays 10, but the uPnL of -11 makes it -1.
    exchange.update_state(&bba(89, 90, 2)).unwrap();

    let settlement = reconcile(&mut exchange);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(
        settlement.forced_cancels[0].limit_price(),
        QuoteCurrency::new(80, 0)
    );
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(exchange.account().active_limit_orders().num_active(), 0);
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(1, 0)
    );
}

#[test]
fn cross_margin_call_keeps_orders_which_unrealized_profit_covers_exactly() {
    // Wallet 100 at 2x: long 1 @ 100 (IM 50), then the bid rises to 119 (uPnL +19).
    let mut exchange = build_cross_linear(
        fee_free_linear_spec(leverage!(2)),
        QuoteCurrency::new(100, 0),
    );
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    exchange.update_state(&bba(119, 120, 2)).unwrap();
    // IM 69 leaves a wallet-only excess of -19, which the uPnL raises to exactly 0.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(138, 2),
            )
            .unwrap(),
        )
        .unwrap();

    let settlement = reconcile(&mut exchange);
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(exchange.account().active_limit_orders().num_active(), 1);
}

/// Long 1 @ 100 at 2x (IM 50), then the bids vanish and leave a one-sided book.
fn long_with_missing_bid(wallet: QuoteCurrency<i64, DECIMALS>) -> LinearExchange {
    let mut exchange = build_cross_linear(fee_free_linear_spec(leverage!(2)), wallet);
    exchange.update_state(&bba(99, 100, 1)).unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();
    exchange.set_best_bid_and_ask(QuoteCurrency::zero(), QuoteCurrency::new(100, 0));
    assert!(!exchange.market_state().has_positive_quotes());
    exchange
}

#[test]
fn cross_margin_does_not_mark_a_long_to_a_missing_bid() {
    // Marking the long to a bid of 0 would book a loss of 100 and wipe out the wallet.
    let mut exchange = long_with_missing_bid(QuoteCurrency::new(100, 0));
    let settlement = reconcile(&mut exchange);
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(1, 0)
    );
}

#[test]
fn cross_margin_without_quotes_checks_maintenance_margin_against_the_entry_notional() {
    let mut exchange = long_with_missing_bid(QuoteCurrency::new(50, 0));
    assert_eq!(reconcile(&mut exchange).solvency, Solvency::Solvent);

    // The long pays 25 of funding: equity 25 still covers the MM of 100 * 0.25 = 25,
    // so it keeps the position, but no longer covers its initial margin of 50.
    let settlement = exchange.apply_funding(
        FundingRate::from(Decimal::try_from_scaled(25, 2).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(100, 0),
    );
    assert_eq!(settlement.cashflow, -QuoteCurrency::new(25, 0));
    assert_eq!(settlement.solvency, Solvency::InitialMarginDeficit);
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(1, 0)
    );
}
