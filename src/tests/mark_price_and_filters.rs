//! Tests for mark-price valuation (`MarkMethod`), `NotionalFilter`, signed maker
//! rebates, and liquidation clearance fees.

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

fn build_linear_exchange(
    spec: ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>,
    starting_balance: QuoteCurrency<i64, DECIMALS>,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let config = Config::new(
        starting_balance,
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
    .expect(EXPECT_CONFIG);
    Exchange::new(config)
}

#[test]
fn mark_method_fair_price_prevents_wick_liquidation_and_tracks_fair_unrealized_pnl() {
    let spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL), // mm = 0.10
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_mark_method(MarkMethod::FairPrice);

    let mut exchange = build_linear_exchange(spec, QuoteCurrency::new(1000, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    exchange
        .update_state(&MarkPriceUpdate::new(QuoteCurrency::new(100, 0), 2.into()))
        .unwrap();

    // Open long 2 @ 100. Entry = 100, liquidation threshold = 100 * (1 - 0.10) = 90.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap())
        .unwrap();
    assert_eq!(exchange.mark_price(), QuoteCurrency::new(100, 0));
    assert_eq!(exchange.unrealized_pnl(), QuoteCurrency::new(0, 0));

    // Order book wicks down to bid=85/ask=86, but external fair price stays at 95 -> NOT liquidated.
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(85, 0),
            ask: QuoteCurrency::new(86, 0),
            timestamp_exchange_ns: 3.into(),
        })
        .unwrap();
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(2, 0)
    );
    // Unrealized PnL uses fair price 100 still (mark_price unchanged by Bba).
    assert_eq!(exchange.unrealized_pnl(), QuoteCurrency::new(0, 0));

    // Fair price drops to 95 -> unrealized PnL = (95 - 100) * 2 = -10, still solvent.
    exchange
        .update_state(&MarkPriceUpdate::new(QuoteCurrency::new(95, 0), 4.into()))
        .unwrap();
    assert_eq!(exchange.unrealized_pnl(), QuoteCurrency::new(-10, 0));

    // Fair price drops to 89 (< 90) -> triggers liquidation!
    let res = exchange.update_state(&MarkPriceUpdate::new(QuoteCurrency::new(89, 0), 5.into()));
    assert_eq!(res, Err(RiskError::Liquidate));
    assert!(exchange.account().position().quantity().is_zero());
}

#[test]
fn notional_filter_rejects_small_open_orders_but_allows_strictly_reducing_dust_close() {
    let notional_filter = NotionalFilter::new(
        Some(QuoteCurrency::new(50, 0)),
        Some(QuoteCurrency::new(500, 0)),
    )
    .unwrap();
    let spec = ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_notional_filter(notional_filter);

    let mut exchange = build_linear_exchange(spec, QuoteCurrency::new(1000, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();

    // 0.4 * 100 = 40 < 50 -> rejected.
    let err = exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(4, 1)).unwrap())
        .unwrap_err();
    assert_eq!(
        err,
        SubmitMarketOrderError::ValidateOrderNotional(ValidateOrderNotionalError::NotionalTooLow)
    );

    // 6.0 * 100 = 600 > 500 -> rejected.
    let err = exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(6, 0)).unwrap())
        .unwrap_err();
    assert_eq!(
        err,
        SubmitMarketOrderError::ValidateOrderNotional(ValidateOrderNotionalError::NotionalTooHigh)
    );

    // Limit order below min notional -> rejected.
    let err = exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(90, 0),
                BaseCurrency::new(5, 1),
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        err,
        SubmitLimitOrderError::ValidateOrderNotional(ValidateOrderNotionalError::NotionalTooLow)
    );

    // Valid open of 1.0 @ 100 (notional = 100).
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap())
        .unwrap();

    // Close 0.7 -> leaves 0.3 dust (notional 29.7 < 50).
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(7, 1)).unwrap())
        .unwrap();
    // Strictly reducing dust close of 0.3 succeeds despite notional < 50.
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(3, 1)).unwrap())
        .unwrap();
    assert!(exchange.account().position().quantity().is_zero());
}

#[test]
fn signed_maker_rebate_credits_equity_and_reserves_zero_fee() {
    let maker_rebate = Fee::from(Decimal::try_from_scaled(-2, 4).expect(EXPECT_DECIMAL)); // -2 bps
    let taker_fee = Fee::from(Decimal::try_from_scaled(5, 4).expect(EXPECT_DECIMAL)); // +5 bps
    let spec = ContractSpecification::new(
        leverage!(2),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        maker_rebate,
        taker_fee,
    )
    .expect(EXPECT_CONTRACT_SPEC);

    let mut exchange = build_linear_exchange(spec, QuoteCurrency::new(1000, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();

    // Rest a buy order at 100 for qty 5 (notional = 500). Rebate reserves 0 maker fee.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(5, 0),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(exchange.account().reserved_maker_fees().is_zero());

    // Sell trade at 99 fills the buy order at 100 -> maker receives rebate of 500 * 0.0002 = 0.1.
    let events = exchange
        .update_state(&Trade {
            timestamp_exchange_ns: 2.into(),
            price: QuoteCurrency::new(99, 0),
            quantity: BaseCurrency::new(5, 0),
            side: Side::Sell,
        })
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(10001, 1)
    );
    assert_eq!(
        exchange.account().balances().total_fees_paid(),
        QuoteCurrency::new(-1, 1)
    );
}

#[test]
fn liquidation_clearance_fee_charged_in_addition_to_taker_fee() {
    let taker_fee = Fee::from(Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL)); // 0.1%
    let liq_fee = Decimal::try_from_scaled(5, 3).expect(EXPECT_DECIMAL); // 0.5%
    let spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL), // mm = 10%
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        taker_fee,
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_liquidation_fee(liq_fee)
    .unwrap();

    // Negative taker fee and invalid liquidation fee are rejected.
    assert_eq!(
        ContractSpecification::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>::new(
            leverage!(1),
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
            PriceFilter::default(),
            QuantityFilter::default(),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::try_from_scaled(-1, 4).expect(EXPECT_DECIMAL)),
        )
        .unwrap_err(),
        ConfigError::InvalidTakerFee
    );
    assert_eq!(
        spec.clone()
            .with_liquidation_fee(Decimal::try_from_scaled(-1, 4).expect(EXPECT_DECIMAL))
            .unwrap_err(),
        ConfigError::InvalidLiquidationFee
    );
    assert_eq!(
        spec.clone().with_liquidation_fee(Decimal::ONE).unwrap_err(),
        ConfigError::InvalidLiquidationFee
    );

    let mut exchange = build_linear_exchange(spec, QuoteCurrency::new(1000, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();

    // Open long 10 @ 100 -> entry notional = 1000, taker fee = 1.0 -> equity = 999.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap())
        .unwrap();
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(999, 0)
    );

    // Bid drops to 89 (< 90 liquidation threshold) -> force_liquidate sells 10 @ 89.
    // Realized PnL = -110. Exit notional = 890.
    // Fee = 890 * (0.001 + 0.005) = 890 * 0.006 = 5.34.
    // Final equity = 999 - 110 - 5.34 = 883.66.
    let res = exchange.update_state(&Bba {
        bid: QuoteCurrency::new(89, 0),
        ask: QuoteCurrency::new(90, 0),
        timestamp_exchange_ns: 2.into(),
    });
    assert_eq!(res, Err(RiskError::Liquidate));
    assert_eq!(
        exchange.account().balances().equity(),
        QuoteCurrency::new(88366, 2)
    );
    assert_eq!(
        exchange.account().balances().total_fees_paid(),
        QuoteCurrency::new(634, 2)
    );
}
