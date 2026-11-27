//! Integration tests for perpetual futures funding rate settlements across
//! linear contracts, inverse contracts, margin calls, liquidations, bankruptcy,
//! high decimal precision, and invalid inputs.

use std::num::NonZeroU16;

use const_decimal::Decimal;

use crate::{
    DECIMALS,
    EXPECT_CONFIG,
    EXPECT_CONTRACT_SPEC,
    EXPECT_DECIMAL,
    EXPECT_NON_ZERO,
    EXPECT_QUANTITY_FILTER,
    mock_exchange_inverse,
    mock_exchange_linear,
    prelude::*,
    utils::NoUserOrderId,
};

fn init_linear_at(
    bid: QuoteCurrency<i64, DECIMALS>,
    ask: QuoteCurrency<i64, DECIMALS>,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let mut exchange = mock_exchange_linear();
    assert!(
        exchange
            .update_state(&Bba {
                bid,
                ask,
                timestamp_exchange_ns: 0.into(),
            })
            .unwrap()
            .is_empty()
    );
    exchange
}

fn init_inverse_at(
    starting_balance: BaseCurrency<i64, DECIMALS>,
    bid: QuoteCurrency<i64, DECIMALS>,
    ask: QuoteCurrency<i64, DECIMALS>,
) -> Exchange<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId> {
    let mut exchange = mock_exchange_inverse(starting_balance);
    assert!(
        exchange
            .update_state(&Bba {
                bid,
                ask,
                timestamp_exchange_ns: 0.into(),
            })
            .unwrap()
            .is_empty()
    );
    exchange
}

fn rate_d5(integer: i64, scale: u8) -> FundingRate<i64, DECIMALS> {
    FundingRate::from(Decimal::try_from_scaled(integer, scale).expect(EXPECT_DECIMAL))
}

#[test]
fn linear_long_positive_rate_pays_funding() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    let equity_before = exchange.account().balances().equity();
    // 2 * 100 = 200 notional; +0.01 (1%) funding -> long pays 2.
    let settlement = exchange.apply_funding(rate_d5(1, 2), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(-2, 0));
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().equity(),
        equity_before - QuoteCurrency::new(2, 0)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(-2, 0)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());
}

#[test]
fn linear_short_positive_rate_receives_funding() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    let equity_before = exchange.account().balances().equity();
    let settlement = exchange.apply_funding(rate_d5(1, 2), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(2, 0));
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().equity(),
        equity_before + QuoteCurrency::new(2, 0)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(2, 0)
    );
}

#[test]
fn linear_long_negative_rate_receives_funding() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    let equity_before = exchange.account().balances().equity();
    let settlement = exchange.apply_funding(rate_d5(-1, 2), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(2, 0));
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().equity(),
        equity_before + QuoteCurrency::new(2, 0)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(2, 0)
    );
}

#[test]
fn linear_short_negative_rate_pays_funding() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();

    let equity_before = exchange.account().balances().equity();
    let settlement = exchange.apply_funding(rate_d5(-1, 2), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(-2, 0));
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().equity(),
        equity_before - QuoteCurrency::new(2, 0)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(-2, 0)
    );
}

#[test]
fn linear_neutral_position_and_zero_rate_produce_zero_cashflow() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    let initial_equity = exchange.account().balances().equity();

    // Neutral position with non-zero rate.
    let settlement = exchange.apply_funding(rate_d5(5, 2), QuoteCurrency::new(100, 0));
    assert!(settlement.cashflow.is_zero());
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(exchange.account().balances().equity(), initial_equity);
    assert!(exchange.account().balances().cumulative_funding().is_zero());

    // Open position with zero rate.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    let equity_after_open = exchange.account().balances().equity();

    let settlement = exchange.apply_funding(FundingRate::default(), QuoteCurrency::new(100, 0));
    assert!(settlement.cashflow.is_zero());
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(exchange.account().balances().equity(), equity_after_open);
    assert!(exchange.account().balances().cumulative_funding().is_zero());
}

#[test]
fn linear_multiple_settlements_accumulate_and_use_mark_price_not_entry_price() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    // Enter long 2 @ entry_price = 101.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    let equity_after_open = exchange.account().balances().equity();

    // Price rises to 200; funding uses mark_price = 200 (not entry_price = 101):
    // 2 * 200 * 0.005 = 2.0 paid.
    let s1 = exchange.apply_funding(rate_d5(5, 3), QuoteCurrency::new(200, 0));
    assert_eq!(s1.cashflow, QuoteCurrency::new(-2, 0));
    assert_eq!(s1.solvency, Solvency::Solvent);

    // Next funding at mark_price = 150 with rate -0.01:
    // 2 * 150 * 0.01 = 3.0 received.
    let s2 = exchange.apply_funding(rate_d5(-1, 2), QuoteCurrency::new(150, 0));
    assert_eq!(s2.cashflow, QuoteCurrency::new(3, 0));
    assert_eq!(s2.solvency, Solvency::Solvent);

    // Net cumulative funding = -2 + 3 = +1.
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(1, 0)
    );
    assert_eq!(
        exchange.account().balances().equity(),
        equity_after_open + QuoteCurrency::new(1, 0)
    );
}

#[test]
fn inverse_funding_settlement_signs_and_mark_price() {
    let mut exchange = init_inverse_at(
        BaseCurrency::new(10, 0),
        QuoteCurrency::new(1000, 0),
        QuoteCurrency::new(1001, 0),
    );

    // Long 2000 Quote @ 1001 => ~1.998 Base notional at entry.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, QuoteCurrency::new(2000, 0)).unwrap())
        .unwrap();
    let equity_after_open = exchange.account().balances().equity();

    // Mark price moves to 2000 => signed base value = 2000 / 2000 = 1 Base (independent of entry price).
    // Rate +0.01 => long pays 0.01 Base.
    let s1 = exchange.apply_funding(rate_d5(1, 2), QuoteCurrency::new(2000, 0));
    assert_eq!(s1.cashflow, BaseCurrency::new(-1, 2));
    assert_eq!(s1.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().equity(),
        equity_after_open - BaseCurrency::new(1, 2)
    );

    // Negative rate -0.02 at mark price 2000 => long receives 0.02 Base.
    let s2 = exchange.apply_funding(rate_d5(-2, 2), QuoteCurrency::new(2000, 0));
    assert_eq!(s2.cashflow, BaseCurrency::new(2, 2));
    assert_eq!(s2.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        BaseCurrency::new(1, 2)
    );

    // Flip to short 2000 Quote @ 1000.
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, QuoteCurrency::new(4000, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.account().position().quantity(),
        QuoteCurrency::new(-2000, 0)
    );

    // Short + positive rate +0.01 at mark 2000 => short receives 0.01 Base.
    let s3 = exchange.apply_funding(rate_d5(1, 2), QuoteCurrency::new(2000, 0));
    assert_eq!(s3.cashflow, BaseCurrency::new(1, 2));
    assert_eq!(s3.solvency, Solvency::Solvent);

    // Short + negative rate -0.01 at mark 2000 => short pays 0.01 Base.
    let s4 = exchange.apply_funding(rate_d5(-1, 2), QuoteCurrency::new(2000, 0));
    assert_eq!(s4.cashflow, BaseCurrency::new(-1, 2));
    assert_eq!(s4.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        BaseCurrency::new(1, 2)
    );
}

#[test]
fn funding_payment_triggers_margin_call_forced_cancel() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));

    // Long 9.7 @ 101: position margin 979.7, taker fee 0.58782, equity 999.41218.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Resting sell limit at 103 for 9.7:
    // order margin = max(0, 999.1 - 979.7) = 19.4, reserved maker fee = 0.19982,
    // required collateral = 999.29982 -> excess = 0.11236.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(103, 0),
                BaseCurrency::new(97, 1),
            )
            .unwrap(),
        )
        .unwrap();

    // Paying funding of 9.7 * 100 * 0.001 = 0.97 drops equity to 998.44218:
    // still above position margin (979.7) and maintenance margin (489.85),
    // but below required collateral (999.29982), so the resting ask is force-cancelled.
    let settlement = exchange.apply_funding(rate_d5(1, 3), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(-97, 2));
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(
        settlement.forced_cancels[0].limit_price(),
        QuoteCurrency::new(103, 0)
    );
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(97, 1)
    );
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(99_844_218, 5)
    );
}

#[test]
fn funding_payment_causes_initial_margin_deficit_and_positive_cashflow_restores_solvency() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));

    // Long 9.7 @ 101: position margin 979.7, maintenance margin 489.85, equity 999.41218.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Funding rate +0.05 at mark 100 -> cashflow = -48.5.
    // Equity drops to 950.91218: below position initial margin (979.7) with no resting orders,
    // yet well above maintenance margin (489.85).
    let deficit = exchange.apply_funding(rate_d5(5, 2), QuoteCurrency::new(100, 0));

    assert_eq!(deficit.cashflow, QuoteCurrency::new(-485, 1));
    assert!(deficit.forced_cancels.is_empty());
    assert_eq!(deficit.solvency, Solvency::InitialMarginDeficit);
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(97, 1)
    );
    assert!(exchange.account().available_balance().is_zero());

    // Zero funding while still in InitialMarginDeficit preserves the deficit status cleanly.
    let zero_step = exchange.apply_funding(FundingRate::default(), QuoteCurrency::new(100, 0));
    assert!(zero_step.cashflow.is_zero());
    assert!(zero_step.forced_cancels.is_empty());
    assert_eq!(zero_step.solvency, Solvency::InitialMarginDeficit);

    // Receiving funding of +48.5 restores equity to 999.41218 >= 979.7 -> Solvent again.
    let recovered = exchange.apply_funding(rate_d5(-5, 2), QuoteCurrency::new(100, 0));

    assert_eq!(recovered.cashflow, QuoteCurrency::new(485, 1));
    assert!(recovered.forced_cancels.is_empty());
    assert_eq!(recovered.solvency, Solvency::Solvent);
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(99_941_218, 5)
    );
    assert!(exchange.account().balances().cumulative_funding().is_zero());
    assert!(exchange.account().available_balance() > QuoteCurrency::zero());
}

#[test]
fn funding_payment_breaching_maintenance_margin_liquidates_position_and_cancels_orders() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));

    // Long 9.7 @ 101: position notional 979.7, maintenance margin 489.85, equity 999.41218.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(103, 0),
                BaseCurrency::new(97, 1),
            )
            .unwrap(),
        )
        .unwrap();

    // Rate +0.55 at mark 100 -> cashflow = -533.5.
    // Equity drops to 465.91218 < 489.85 maintenance margin -> force_liquidate() runs:
    // cancels resting ask, closes 9.7 @ bid 100 (realized pnl -9.7, taker fee 0.582).
    // Final equity = 465.91218 - 9.7 - 0.582 = 455.63018 (> 0, so Liquidated not Bankrupt).
    let settlement = exchange.apply_funding(rate_d5(55, 2), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(-5335, 1));
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(45_563_018, 5)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());
}

#[test]
fn adverse_funding_exhausting_equity_bankrupts_account_and_records_bad_debt() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));

    // Long 9.7 @ 101: equity 999.41218.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Extreme funding rate +1.10 at mark 100 -> cashflow = -1067.0, exceeding equity 999.41218.
    // Funding creates 67.58782 bad debt and zeroes equity; force_liquidate() closes 9.7 @ 100
    // adding realized loss 9.7 and taker fee 0.582 -> total bad debt = 77.86982.
    let settlement = exchange.apply_funding(rate_d5(11, 1), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, QuoteCurrency::new(-1067, 0));
    assert_eq!(settlement.solvency, Solvency::Bankrupt);
    assert!(settlement.forced_cancels.is_empty());
    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().balances().equity().is_zero());
    assert_eq!(
        exchange.account().balances().bad_debt(),
        QuoteCurrency::new(7_786_982, 5)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(-1067, 0)
    );
}

#[test]
#[should_panic(expected = "funding mark price must be strictly positive")]
fn zero_mark_price_panics() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    let _ = exchange.apply_funding(rate_d5(1, 4), QuoteCurrency::zero());
}

#[test]
#[should_panic(expected = "funding mark price must be strictly positive")]
fn negative_mark_price_panics() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    let _ = exchange.apply_funding(rate_d5(1, 4), QuoteCurrency::new(-100, 0));
}

#[test]
fn high_precision_d8_binance_btcusdt_funding() {
    const D8: u8 = 8;

    let contract_spec = ContractSpecification::<i64, D8, BaseCurrency<i64, D8>>::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 3)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::try_from_scaled(2, 4).expect(EXPECT_DECIMAL)),
        Fee::from(Decimal::try_from_scaled(5, 4).expect(EXPECT_DECIMAL)),
    )
    .expect(EXPECT_CONTRACT_SPEC);
    let config = Config::new(
        QuoteCurrency::new(50_000, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        contract_spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    let mut exchange = Exchange::<i64, D8, BaseCurrency<i64, D8>, NoUserOrderId>::new(config);

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(64_999, 0),
                ask: QuoteCurrency::new(65_000, 0),
                timestamp_exchange_ns: 0.into(),
            })
            .unwrap()
            .is_empty()
    );

    // Long 1.5 BTC @ 65,000 USDT = 97,500 USDT notional.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(15, 1)).unwrap())
        .unwrap();

    // Standard Binance 1 bp funding rate (0.0001): 97,500 * 0.0001 = 9.75 USDT paid.
    let rate_1bp = FundingRate::from(Decimal::<i64, D8>::try_from_scaled(1, 4).unwrap());
    let s1 = exchange.apply_funding(rate_1bp, QuoteCurrency::new(65_000, 0));
    assert_eq!(s1.cashflow, QuoteCurrency::new(-975, 2));
    assert_eq!(s1.solvency, Solvency::Solvent);

    // Small funding rate 0.0000125 (0.125 bp): 97,500 * 0.0000125 = 1.21875 USDT paid.
    let rate_tiny = FundingRate::from(Decimal::<i64, D8>::try_from_scaled(125, 7).unwrap());
    let s2 = exchange.apply_funding(rate_tiny, QuoteCurrency::new(65_000, 0));
    assert_eq!(s2.cashflow, QuoteCurrency::new(-121_875, 5));
    assert_eq!(s2.solvency, Solvency::Solvent);

    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        QuoteCurrency::new(-1_096_875, 5)
    );
}

#[test]
fn inverse_funding_payment_triggers_margin_call_forced_cancel() {
    let mut exchange = init_inverse_at(
        BaseCurrency::new(10, 0),
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(101, 0),
    );

    // Long 970 Quote @ ask 101:
    // position notional = 970 / 101 = 9.60396 Base (truncated to 5 decimals),
    // taker fee = 9.60396 * 0.0006 = 0.00576 Base, equity = 9.99424 Base.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, QuoteCurrency::new(970, 0)).unwrap())
        .unwrap();
    let position_notional = exchange.account().position().notional();
    assert_eq!(position_notional, BaseCurrency::new(960_396, 5));

    // Resting sell limit at 101 (passive, above bid 100) for 1009 Quote:
    // order notional = 1009 / 101 = 9.99009 Base,
    // order margin = max(0, 9.99009 - 9.60396) = 0.38613 Base,
    // reserved maker fee = 9.99009 * 0.0002 = 0.00199 Base,
    // required collateral = 9.99208 Base <= equity 9.99424 Base -> accepted.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(101, 0),
                QuoteCurrency::new(1009, 0),
            )
            .unwrap(),
        )
        .unwrap();

    // Paying funding at mark 100 with rate +0.001:
    // signed base value = 970 / 100 = 9.7 Base -> cashflow = -0.00970 Base.
    // Equity drops to 9.98454 Base: still above position margin (9.60396) and maintenance
    // margin (4.80198), but below required collateral (9.99208), so the resting ask is cancelled.
    let settlement = exchange.apply_funding(rate_d5(1, 3), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, BaseCurrency::new(-970, 5));
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(
        settlement.forced_cancels[0].limit_price(),
        QuoteCurrency::new(101, 0)
    );
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().position().quantity(),
        QuoteCurrency::new(970, 0)
    );
    assert_eq!(
        exchange.account().balances().equity(),
        BaseCurrency::new(998_454, 5)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        BaseCurrency::new(-970, 5)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());
}

#[test]
fn inverse_funding_payment_triggers_initial_margin_deficit_and_liquidation() {
    let mut exchange = init_inverse_at(
        BaseCurrency::new(10, 0),
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(101, 0),
    );

    // Long 970 Quote @ ask 101:
    // position notional = 9.60396 Base, maintenance margin = 4.80198 Base, equity = 9.99424 Base.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, QuoteCurrency::new(970, 0)).unwrap())
        .unwrap();

    // Rate +0.05 at mark 100 -> cashflow = -(9.7 * 0.05) = -0.485 Base.
    // Equity drops to 9.50924 Base < position margin (9.60396), > maintenance margin (4.80198).
    let deficit = exchange.apply_funding(rate_d5(5, 2), QuoteCurrency::new(100, 0));

    assert_eq!(deficit.cashflow, BaseCurrency::new(-48_500, 5));
    assert!(deficit.forced_cancels.is_empty());
    assert_eq!(deficit.solvency, Solvency::InitialMarginDeficit);
    assert!(exchange.account().available_balance().is_zero());

    // Now push equity below maintenance margin with another funding payment:
    // rate +0.50 at mark 100 -> cashflow = -(9.7 * 0.50) = -4.85 Base.
    // Equity after funding = 9.50924 - 4.85 = 4.65924 Base < 4.80198 -> force_liquidate().
    // Closing Long 970 Quote @ bid 100:
    // inverse pnl = 970/101 - 970/100 = 9.60396 - 9.7 = -0.09604 Base,
    // taker fee = (970 / 100) * 0.0006 = 0.00582 Base.
    // Final equity = 4.65924 - 0.09604 - 0.00582 = 4.55738 Base.
    let liq = exchange.apply_funding(rate_d5(5, 1), QuoteCurrency::new(100, 0));

    assert_eq!(liq.cashflow, BaseCurrency::new(-485_000, 5));
    assert_eq!(liq.solvency, Solvency::Liquidated);
    assert!(liq.forced_cancels.is_empty());
    assert!(exchange.account().position().quantity().is_zero());
    assert_eq!(
        exchange.account().balances().equity(),
        BaseCurrency::new(455_738, 5)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        BaseCurrency::new(-533_500, 5)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());
}

#[test]
fn inverse_adverse_funding_exhausting_equity_bankrupts_account() {
    let mut exchange = init_inverse_at(
        BaseCurrency::new(10, 0),
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(101, 0),
    );

    // Long 970 Quote @ ask 101: equity = 9.99424 Base.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, QuoteCurrency::new(970, 0)).unwrap())
        .unwrap();
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(101, 0),
                QuoteCurrency::new(1009, 0),
            )
            .unwrap(),
        )
        .unwrap();

    // Extreme rate +1.10 at mark 100 -> cashflow = -(9.7 * 1.10) = -10.67 Base.
    // Funding creates 0.67576 Base bad debt and zeroes equity; force_liquidate() cancels the
    // resting ask and closes Long 970 @ bid 100 (pnl -0.09604, taker fee 0.00582),
    // bringing total bad debt to 0.77762 Base.
    let settlement = exchange.apply_funding(rate_d5(11, 1), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, BaseCurrency::new(-1_067_000, 5));
    assert_eq!(settlement.solvency, Solvency::Bankrupt);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().active_limit_orders().is_empty());
    assert!(exchange.account().balances().equity().is_zero());
    assert_eq!(
        exchange.account().balances().bad_debt(),
        BaseCurrency::new(77_762, 5)
    );
    assert_eq!(
        exchange.account().balances().cumulative_funding(),
        BaseCurrency::new(-1_067_000, 5)
    );
}

#[test]
fn inverse_short_negative_funding_triggers_liquidation() {
    let mut exchange = init_inverse_at(
        BaseCurrency::new(10, 0),
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(101, 0),
    );

    // Short 970 Quote @ bid 100:
    // position notional = 970 / 100 = 9.7 Base, maintenance margin = 4.85 Base,
    // taker fee = 9.7 * 0.0006 = 0.00582 Base, equity = 9.99418 Base.
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, QuoteCurrency::new(970, 0)).unwrap())
        .unwrap();
    // Resting passive buy limit at 100 for 998 Quote:
    // order notional = 998 / 100 = 9.98 Base, order margin = 0.28 Base,
    // reserved maker fee = 0.00199 Base, required collateral = 9.98199 Base <= 9.99418 Base.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                QuoteCurrency::new(998, 0),
            )
            .unwrap(),
        )
        .unwrap();

    // Negative rate -0.55 at mark 100 -> short pays 9.7 * 0.55 = 5.335 Base.
    // Equity drops to 4.65918 Base < 4.85 maintenance margin -> force_liquidate() cancels
    // the resting bid and closes Short 970 @ ask 101:
    // inverse short pnl = -(970/100 - 970/101) = -(9.7 - 9.60396) = -0.09604 Base,
    // taker fee = 9.60396 * 0.0006 = 0.00576 Base.
    // Final equity = 4.65918 - 0.09604 - 0.00576 = 4.55738 Base.
    let settlement = exchange.apply_funding(rate_d5(-55, 2), QuoteCurrency::new(100, 0));

    assert_eq!(settlement.cashflow, BaseCurrency::new(-533_500, 5));
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances().equity(),
        BaseCurrency::new(455_738, 5)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());
}

#[test]
fn short_position_negative_funding_triggers_margin_call_liquidation_and_bankruptcy() {
    // 1) Margin call forced cancel on short with negative funding rate.
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    // Short 9.7 @ bid 100: position margin 970, taker fee 0.582, equity 999.418.
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();
    // Resting buy limit at 100 (passive, below ask 101) for 9.98:
    // notional = 998, order margin = max(0, 998 - 970) = 28,
    // reserved maker fee = 0.1996, required collateral = 998.1996 <= 999.418.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(998, 2),
            )
            .unwrap(),
        )
        .unwrap();

    // Negative rate -0.002 at mark 100 -> short pays 9.7 * 100 * 0.002 = 1.94.
    // Equity drops to 997.478 < 998.1996 -> resting bid is force-cancelled.
    let margin_call = exchange.apply_funding(rate_d5(-2, 3), QuoteCurrency::new(100, 0));
    assert_eq!(margin_call.cashflow, QuoteCurrency::new(-194, 2));
    assert_eq!(margin_call.solvency, Solvency::Solvent);
    assert_eq!(margin_call.forced_cancels.len(), 1);
    assert!(exchange.account().active_limit_orders().is_empty());

    // 2) Liquidation on short with negative funding rate (breaching maintenance margin 485).
    // Equity is 997.478; rate -0.55 at mark 100 -> cashflow = -533.5 -> equity 463.978 < 485.
    // force_liquidate() closes Short 9.7 @ ask 101: pnl = -9.7, taker fee = 0.58782.
    // Final equity = 463.978 - 9.7 - 0.58782 = 453.69018 (> 0 => Liquidated).
    let liq = exchange.apply_funding(rate_d5(-55, 2), QuoteCurrency::new(100, 0));
    assert_eq!(liq.cashflow, QuoteCurrency::new(-5335, 1));
    assert_eq!(liq.solvency, Solvency::Liquidated);
    assert!(exchange.account().position().quantity().is_zero());
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(45_369_018, 5)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());

    // 3) Bankruptcy on short with extreme negative funding rate.
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();
    // Equity 999.418; rate -1.10 at mark 100 -> cashflow = -1067.0.
    // Funding bad debt = 67.582; liquidation @ ask 101 adds pnl 9.7 + fee 0.58782 -> 77.86982.
    let bankrupt = exchange.apply_funding(rate_d5(-11, 1), QuoteCurrency::new(100, 0));
    assert_eq!(bankrupt.cashflow, QuoteCurrency::new(-1067, 0));
    assert_eq!(bankrupt.solvency, Solvency::Bankrupt);
    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().balances().equity().is_zero());
    assert_eq!(
        exchange.account().balances().bad_debt(),
        QuoteCurrency::new(7_786_982, 5)
    );
}

#[test]
fn subsequent_zero_cashflow_after_bankruptcy_does_not_re_report_bankrupt() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    let bankrupt = exchange.apply_funding(rate_d5(11, 1), QuoteCurrency::new(100, 0));
    assert_eq!(bankrupt.solvency, Solvency::Bankrupt);
    let bad_debt = exchange.account().balances().bad_debt();
    assert!(bad_debt > QuoteCurrency::zero());

    // Position is now flat and equity is zero, so a subsequent funding tick produces zero
    // cashflow and no new bad debt -> reported as Solvent rather than re-triggering Bankrupt.
    let next = exchange.apply_funding(rate_d5(1, 2), QuoteCurrency::new(100, 0));
    assert!(next.cashflow.is_zero());
    assert!(next.forced_cancels.is_empty());
    assert_eq!(next.solvency, Solvency::Solvent);
    assert_eq!(exchange.account().balances().bad_debt(), bad_debt);
}

#[test]
fn tiny_rate_truncating_to_zero_cashflow_leaves_balances_unchanged() {
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    // Long 0.01 @ 101 => notional at mark 0.01 is 0.00010 Quote.
    // Multiply by 0.00001 (1e-5) -> 1e-9, which truncates to 0 at D=5.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 2)).unwrap())
        .unwrap();
    let equity_before = exchange.account().balances().equity();

    let settlement = exchange.apply_funding(rate_d5(1, 5), QuoteCurrency::new(1, 2));

    assert!(settlement.cashflow.is_zero());
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(exchange.account().balances().equity(), equity_before);
    assert!(exchange.account().balances().cumulative_funding().is_zero());
}

#[test]
fn zero_rate_funding_reconciles_existing_collateral_shortfall() {
    // When an account is already in `InitialMarginDeficit` (e.g. from an earlier funding
    // outflow) and a reduce-side order is no longer present, a zero-rate funding call still
    // runs `reconcile_margin` and reports the current solvency. If an un-reconciled debit
    // existed while orders were resting (testable via `Exchange::account_mut` in unit tests),
    // zero-rate funding reconciles it immediately.
    let mut exchange = init_linear_at(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(103, 0),
                BaseCurrency::new(97, 1),
            )
            .unwrap(),
        )
        .unwrap();

    // Direct `Account` debit without reconciliation leaves equity below required collateral
    // while the resting sell limit is still active; calling `apply_funding` with a zero rate
    // must still run `reconcile_margin` and force-cancel the order.
    let cf = exchange
        .account()
        .funding_cashflow(rate_d5(1, 3), QuoteCurrency::new(100, 0));
    assert_eq!(cf, QuoteCurrency::new(-97, 2));
    exchange.account_mut().apply_funding(cf);
    assert!(exchange.account().margin_excess() < QuoteCurrency::zero());

    let settlement = exchange.apply_funding(FundingRate::default(), QuoteCurrency::new(100, 0));
    assert!(settlement.cashflow.is_zero());
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert!(exchange.account().active_limit_orders().is_empty());

    // Likewise, if equity is already below maintenance margin on a zero-rate tick,
    // `apply_funding` force-liquidates the position.
    exchange
        .account_mut()
        .apply_funding(QuoteCurrency::new(-5335, 1));
    let liq = exchange.apply_funding(FundingRate::default(), QuoteCurrency::new(100, 0));
    assert!(liq.cashflow.is_zero());
    assert_eq!(liq.solvency, Solvency::Liquidated);
    assert!(exchange.account().position().quantity().is_zero());
}

/// Fee-free isolated exchange at bid/ask 99/100 holding long 5 @ 100: the entry notional of
/// 500 needs a maintenance margin of 250 (1x leverage, maintenance fraction 0.5).
fn isolated_long_5_at_100() -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::default(),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC);
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    assert_eq!(config.margin_mode(), MarginMode::Isolated);
    let mut exchange = Exchange::new(config);
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(5, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(1000, 0)
    );
    exchange
}

#[test]
fn isolated_equity_exactly_at_the_maintenance_margin_is_not_liquidated() {
    // Paying 150% of the 500 notional leaves exactly the maintenance margin of 250.
    let mut exchange = isolated_long_5_at_100();
    let settlement = exchange.apply_funding(rate_d5(15, 1), QuoteCurrency::new(100, 0));
    assert_eq!(settlement.cashflow, QuoteCurrency::new(-750, 0));
    // Below the initial margin of 500, but not liquidated.
    assert_eq!(settlement.solvency, Solvency::InitialMarginDeficit);
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(5, 0)
    );

    // A cent more of funding falls below it.
    let mut exchange = isolated_long_5_at_100();
    let settlement = exchange.apply_funding(rate_d5(150_001, 5), QuoteCurrency::new(100, 0));
    assert_eq!(settlement.solvency, Solvency::Liquidated);
    assert!(exchange.account().position().quantity().is_zero());
}
