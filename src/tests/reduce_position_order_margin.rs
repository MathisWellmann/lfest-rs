//! A position-reducing fill is never rejected by the venue, yet settling it pays fees,
//! may realize a loss and shrinks the position notional which offset resting reduce-side
//! limit orders. These tests assert that the exchange reconciles the resulting collateral
//! shortfall like a real venue: by force-cancelling resting limit orders (margin call),
//! force-closing the position when the maintenance margin is breached and recording bad
//! debt on bankruptcy - never by rejecting the risk-reducing order or panicking.

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
    test_fee_maker,
    test_fee_taker,
    utils::NoUserOrderId,
};

fn setup_long_with_resting_ask()
-> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let mut exchange = mock_exchange_linear();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    // Open a long position with almost the entire wallet:
    // 9.7 @ 101 -> position margin 979.7, taker fee 0.58782, equity 999.41218.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Rest a sell limit order above entry. It is almost fully offset by the long position:
    // order margin max(0, 999.1 - 979.7) = 19.4 plus a maker fee reserve of 0.19982,
    // which passes the canonical admission check.
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

    exchange
}

/// Partially closing the long at a loss is never rejected, but it realizes a loss of 4,
/// pays a taker fee of 0.24 and re-offsets the resting ask from 19.4 to 423.4 of order
/// margin. The required collateral of 999.29982 then exceeds the equity of 995.17218,
/// so the venue force-cancels the resting ask within the same settlement.
#[test]
fn reducing_market_order_triggers_margin_call_cancel() {
    let mut exchange = setup_long_with_resting_ask();

    let settlement = exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap())
        .unwrap();

    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(
        settlement.forced_cancels[0].limit_price(),
        QuoteCurrency::new(103, 0)
    );
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(99_517_218, 5)
    );
    // equity 995.17218 - position margin 575.7; observing it must not panic.
    assert_eq!(
        exchange.account().available_balance(),
        QuoteCurrency::new(41_947_218, 5)
    );

    // The account is fully operational afterwards: a new limit order is accepted.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(104, 0),
                BaseCurrency::new(57, 1),
            )
            .unwrap(),
        )
        .unwrap();
}

/// A liquidation force-cancels the resting orders first (emitting them as events),
/// then closes the position with an internal fill which bypasses the order rate
/// limiter and all admission checks.
#[test]
fn liquidation_force_cancels_resting_orders() {
    let mut exchange = setup_long_with_resting_ask();

    // Crash below the liquidation price of 101 * (1 - 0.5) = 50.5.
    // The position is force-closed at the bid of 50:
    // realized pnl 9.7 * (50 - 101) = -494.7, taker fee 0.291.
    let result = exchange.update_state(&Bba {
        bid: QuoteCurrency::new(50, 0),
        ask: QuoteCurrency::new(51, 0),
        timestamp_exchange_ns: 1.into(),
    });
    assert!(matches!(result, Err(RiskError::Liquidate)));

    // The forced cancellation is observable through the event stream.
    assert_eq!(exchange.limit_order_events().len(), 1);
    assert!(matches!(
        exchange.limit_order_events()[0],
        LimitOrderEvent::ForcedCancel(_)
    ));

    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(50_442_118, 5)
    );
    assert!(exchange.account().balances().bad_debt().is_zero());
    assert_eq!(
        exchange.account().available_balance(),
        QuoteCurrency::new(50_442_118, 5)
    );
}

/// The mirrored case: reducing a short position at a loss re-offsets a resting bid.
#[test]
fn reducing_short_market_order_triggers_margin_call_cancel() {
    let mut exchange = mock_exchange_linear();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    // Short 9.7 @ 100 -> position margin 970, taker fee 0.582, equity 999.418.
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Rest a buy limit order at the entry price; fully offset by the short (order margin 0).
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(97, 1),
            )
            .unwrap(),
        )
        .unwrap();

    // The market moves against the short, but stays above the liquidation price of 150.
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(110, 0),
                ask: QuoteCurrency::new(111, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );

    // Reduce the short by 4 @ 111: realized pnl 4 * (100 - 111) = -44, taker fee 0.2664,
    // equity 955.1516. The resting bid is re-offset from 0 to 400 of order margin, so the
    // required collateral of 970.194 exceeds the equity and the bid is cancelled.
    let settlement = exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(4, 0)).unwrap())
        .unwrap();

    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(95_515_160, 5)
    );
    // equity 955.1516 - position margin 570; observing it must not panic.
    assert_eq!(
        exchange.account().available_balance(),
        QuoteCurrency::new(38_515_160, 5)
    );
}

/// The margin call cancels the largest collateral contributor first,
/// so as few orders as possible are cancelled.
#[test]
fn margin_call_cancels_largest_order_first() {
    let mut exchange = mock_exchange_linear();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Two resting asks with a combined notional of 999.1, as in the single-order case.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(103, 0),
                BaseCurrency::new(5, 0),
            )
            .unwrap(),
        )
        .unwrap();
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(103, 0),
                BaseCurrency::new(47, 1),
            )
            .unwrap(),
        )
        .unwrap();

    // The same reducing order as in `reducing_market_order_triggers_margin_call_cancel`:
    // cancelling the larger ask (notional 515) covers the shortfall on its own.
    let settlement = exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap())
        .unwrap();

    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(
        settlement.forced_cancels[0].remaining_quantity(),
        BaseCurrency::new(5, 0)
    );
    // The smaller ask survives as it is fully offset by the remaining position.
    let surviving: Vec<_> = exchange.account().active_limit_orders().iter().collect();
    assert_eq!(surviving.len(), 1);
    assert_eq!(surviving[0].remaining_quantity(), BaseCurrency::new(47, 1));
    // equity 995.17218 - position margin 575.7 - fee reserve 0.09682 of the survivor.
    assert_eq!(
        exchange.account().available_balance(),
        QuoteCurrency::new(41_937_536, 5)
    );
}

/// A leveraged position liquidated after the market gapped through its bankruptcy price
/// realizes a loss larger than the account equity. The venue absorbs the excess as bad
/// debt instead of panicking, mirroring a real insurance fund.
#[test]
fn gapped_liquidation_records_bad_debt_without_panicking() {
    let contract_spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect(EXPECT_CONTRACT_SPEC);
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        contract_spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    let mut exchange =
        Exchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(config);

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    // Long 40 @ 101 at 5x leverage: position margin 808, taker fee 2.424, equity 997.576.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(40, 0)).unwrap())
        .unwrap();

    // The market gaps far below the liquidation price of 101 * (1 - 0.1) = 90.9.
    // Closing 40 @ 70 realizes a pnl of -1240, exceeding the equity of 997.576:
    // 242.424 of the loss plus the 1.68 liquidation fee become bad debt.
    let result = exchange.update_state(&Bba {
        bid: QuoteCurrency::new(70, 0),
        ask: QuoteCurrency::new(71, 0),
        timestamp_exchange_ns: 1.into(),
    });
    assert!(matches!(result, Err(RiskError::Liquidate)));

    assert!(exchange.account().position().quantity().is_zero());
    assert!(exchange.account().balances().equity().is_zero());
    assert_eq!(
        exchange.account().balances().bad_debt(),
        QuoteCurrency::new(24_410_400, 5)
    );
    assert!(exchange.account().available_balance().is_zero());
}
