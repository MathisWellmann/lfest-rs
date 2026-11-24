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

fn liquidation_fee_spec() -> ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL), // mm = 10%
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::try_from_scaled(1, 3).expect(EXPECT_DECIMAL)), // 0.1%
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_liquidation_fee(Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL)) // 5%
    .unwrap()
}

/// Long 10 @ 100 from a 210 wallet: equity 209 after the 1.0 taker fee.
fn long_10_liquidation_fee_exchange()
-> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let mut exchange = build_linear_exchange(liquidation_fee_spec(), QuoteCurrency::new(210, 0));
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
        exchange.account().balances().equity(),
        QuoteCurrency::new(209, 0)
    );
    exchange
}

#[test]
fn liquidation_clearance_fee_is_capped_at_the_equity_left() {
    let mut exchange = long_10_liquidation_fee_exchange();
    // Gap to bid 80: realized pnl = -200, taker fee = 0.8 -> 8.2 left, which caps the
    // 800 * 5% = 40 clearance fee. Without the cap the fee would create 31.8 bad debt.
    let res = exchange.update_state(&Bba {
        bid: QuoteCurrency::new(80, 0),
        ask: QuoteCurrency::new(81, 0),
        timestamp_exchange_ns: 2.into(),
    });
    assert_eq!(res, Err(RiskError::Liquidate));
    let balances = exchange.account().balances();
    assert!(balances.equity().is_zero());
    assert!(balances.bad_debt().is_zero());
    assert_eq!(balances.total_fees_paid(), QuoteCurrency::new(10, 0));
}

#[test]
fn liquidation_clearance_fee_is_waived_when_the_close_already_bankrupts() {
    let mut exchange = long_10_liquidation_fee_exchange();
    // Gap to bid 70: realized pnl = -300, taker fee = 0.7 -> 91.7 bad debt from the
    // close itself; the clearance fee must not add to it.
    let res = exchange.update_state(&Bba {
        bid: QuoteCurrency::new(70, 0),
        ask: QuoteCurrency::new(71, 0),
        timestamp_exchange_ns: 2.into(),
    });
    assert_eq!(res, Err(RiskError::Liquidate));
    let balances = exchange.account().balances();
    assert!(balances.equity().is_zero());
    assert_eq!(balances.bad_debt(), QuoteCurrency::new(917, 1));
}

#[test]
fn portfolio_liquidation_clearance_fee_is_capped_at_the_equity_left() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(210, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            liquidation_fee_spec(),
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
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();
    pf.submit_market_order(
        m,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();

    let res = pf.update_market(
        m,
        &Bba {
            bid: QuoteCurrency::new(80, 0),
            ask: QuoteCurrency::new(81, 0),
            timestamp_exchange_ns: 2.into(),
        },
    );
    assert!(matches!(
        res,
        Err(PortfolioError::Risk(RiskError::Liquidate))
    ));
    assert!(pf.market(m).unwrap().position.quantity().is_zero());
    assert!(pf.balances().equity().is_zero());
    assert!(pf.balances().bad_debt().is_zero());
    assert_eq!(pf.balances().total_fees_paid(), QuoteCurrency::new(10, 0));
}

fn slippage_exchange(
    notional_filter: NotionalFilter<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let spec = mark_price_spec()
        .with_slippage_model(
            SlippageModel::fixed_rate(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL))
                .unwrap(),
        )
        .with_notional_filter(notional_filter);
    let mut exchange = build_linear_exchange(spec, QuoteCurrency::new(1000, 0));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    exchange
}

fn mark_price_spec() -> ContractSpecification<i64, DECIMALS, BaseCurrency<i64, DECIMALS>> {
    ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
}

fn gtc(
    side: Side,
    limit_price: QuoteCurrency<i64, DECIMALS>,
) -> LimitOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder> {
    LimitOrder::new(side, limit_price, BaseCurrency::new(1, 0))
        .unwrap()
        .with_time_in_force(TimeInForce::GoodTilCancelled)
}

fn fill_price(
    submission: LimitOrderSubmission<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>,
) -> QuoteCurrency<i64, DECIMALS> {
    match submission {
        LimitOrderSubmission::ImmediatelyFilled { filled_order, .. } => {
            filled_order.state().avg_fill_price()
        }
        LimitOrderSubmission::Resting(order) => panic!("expected a fill, got resting {order}"),
    }
}

#[test]
fn marketable_limit_fill_applies_slippage_capped_at_limit_price() {
    let mut ex = slippage_exchange(NotionalFilter::default());
    // Ask 101 + 1% = 102.01, within the limit of 105.
    let sub = ex
        .submit_limit_order_with_execution(gtc(Side::Buy, QuoteCurrency::new(105, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(10201, 2));
    // 102.01 is beyond the limit of 102, so the fill is capped there.
    let sub = ex
        .submit_limit_order_with_execution(gtc(Side::Buy, QuoteCurrency::new(102, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(102, 0));
    // Bid 100 - 1% = 99, within the limit of 95.
    let sub = ex
        .submit_limit_order_with_execution(gtc(Side::Sell, QuoteCurrency::new(95, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(99, 0));
    // 99 is beyond the limit of 100, so the fill is capped there.
    let sub = ex
        .submit_limit_order_with_execution(gtc(Side::Sell, QuoteCurrency::new(100, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(100, 0));
}

#[test]
fn marketable_limit_notional_filter_uses_the_fill_price() {
    // Fill at 102.01 is below min 103, although the limit 105 is above it.
    let mut ex =
        slippage_exchange(NotionalFilter::new(Some(QuoteCurrency::new(103, 0)), None).unwrap());
    assert_eq!(
        ex.submit_limit_order_with_execution(gtc(Side::Buy, QuoteCurrency::new(105, 0)))
            .unwrap_err(),
        SubmitLimitOrderError::ValidateOrderNotional(ValidateOrderNotionalError::NotionalTooLow)
    );
    assert!(ex.account().position().quantity().is_zero());

    // Fill at 102.01 is below max 103, although the limit 105 is above it.
    let mut ex =
        slippage_exchange(NotionalFilter::new(None, Some(QuoteCurrency::new(103, 0))).unwrap());
    let sub = ex
        .submit_limit_order_with_execution(gtc(Side::Buy, QuoteCurrency::new(105, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(10201, 2));
}

#[test]
fn portfolio_marketable_limit_fill_applies_slippage_and_validates_fill_notional() {
    let spec = mark_price_spec()
        .with_slippage_model(
            SlippageModel::fixed_rate(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL))
                .unwrap(),
        )
        .with_notional_filter(NotionalFilter::new(None, Some(QuoteCurrency::new(103, 0))).unwrap());
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
    // Limit 105 notional exceeds max 103, the 102.01 fill does not.
    let sub = pf
        .submit_limit_order_with_execution(m, gtc(Side::Buy, QuoteCurrency::new(105, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(10201, 2));
    let sub = pf
        .submit_limit_order_with_execution(m, gtc(Side::Buy, QuoteCurrency::new(102, 0)))
        .unwrap();
    assert_eq!(fill_price(sub), QuoteCurrency::new(102, 0));
    assert_eq!(
        pf.market(m).unwrap().position.quantity(),
        BaseCurrency::new(2, 0)
    );
}
