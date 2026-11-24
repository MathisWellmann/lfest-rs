//! Integration tests for multi-symbol `PortfolioExchange`, cross-margin netting,
//! `MarketId(u16)` indexing, market lifecycle (`ReduceOnly`, `delist_market`), and
//! mass-cancel (`cancel_all_limit_orders`, `cancel_limit_orders_by_side`).

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

fn make_linear_market_cfg(
    ticker: &str,
    lev: u8,
) -> MarketConfig<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>> {
    let spec = ContractSpecification::new(
        Leverage::new(lev).unwrap(),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_ticker(ticker);
    MarketConfig::new(
        NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
        spec,
        OrderRateLimits::default(),
    )
}

#[test]
fn portfolio_cross_margin_winning_leg_offsets_losing_leg_and_prevents_liquidation() {
    // Shared wallet = 200 USDT, 2 markets (BTCUSDT & ETHUSDT), 5x leverage (IM=20%, MM=10%).
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(200, 0),
        vec![
            make_linear_market_cfg("BTCUSDT", 5),
            make_linear_market_cfg("ETHUSDT", 5),
        ],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);

    let btc = pf.market_id_by_ticker("BTCUSDT").unwrap();
    let eth = pf.market_id_by_ticker("ETHUSDT").unwrap();
    assert_eq!(btc, MarketId::new(0));
    assert_eq!(eth, MarketId::new(1));
    assert_eq!(pf.market_count(), 2);

    pf.update_market(
        btc,
        &Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();
    pf.update_market(
        eth,
        &Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();

    // Long 4 BTC @ 100 (notional 400, IM 80) AND Short 4 ETH @ 100 (notional 400, IM 80).
    pf.submit_market_order(
        btc,
        MarketOrder::new(Side::Buy, BaseCurrency::new(4, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_market_order(
        eth,
        MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap(),
    )
    .unwrap();

    // Market rallies: ETH rallies +45 to 145/146 (short ETH loses 4 * 46 = -184 -> would blow up an isolated 100-budget sleeve!),
    // while BTC also rallies +45 to 145/146 (long BTC gains 4 * 45 = +180).
    pf.update_market(
        btc,
        &Bba {
            bid: QuoteCurrency::new(145, 0),
            ask: QuoteCurrency::new(146, 0),
            timestamp_exchange_ns: 2.into(),
        },
    )
    .unwrap();
    pf.update_market(
        eth,
        &Bba {
            bid: QuoteCurrency::new(145, 0),
            ask: QuoteCurrency::new(146, 0),
            timestamp_exchange_ns: 2.into(),
        },
    )
    .unwrap();

    // Net uPnL = +180 - 184 = -4, total equity = 196, total MM = (580 + 584) * 0.10 = 116.4 -> SOLVENT!
    assert_eq!(pf.total_unrealized_pnl(), QuoteCurrency::new(-4, 0));
    assert_eq!(pf.total_equity(), QuoteCurrency::new(196, 0));
    assert!(!pf.market(btc).unwrap().position.quantity().is_zero());
    assert!(!pf.market(eth).unwrap().position.quantity().is_zero());

    // Now BTC drops back to 100 while ETH stays at 146 -> uPnL = 0 - 184 = -184 -> equity = 16 < MM (~98.4) -> LIQUIDATE ALL!
    let res = pf.update_market(
        btc,
        &Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 3.into(),
        },
    );
    assert_eq!(res, Err(RiskError::Liquidate));
    assert!(pf.market(btc).unwrap().position.quantity().is_zero());
    assert!(pf.market(eth).unwrap().position.quantity().is_zero());
}

#[test]
fn portfolio_reduce_only_and_delist_market_settle_position_and_cancel_orders() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![make_linear_market_cfg("FTMUSDT", 2)],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    let ftm = pf.market_id_by_ticker("FTMUSDT").unwrap();

    // Also test dynamically adding a second market.
    let matic = pf
        .add_market(make_linear_market_cfg("MATICUSDT", 2))
        .unwrap();
    assert_eq!(matic, MarketId::new(1));

    pf.update_market(
        ftm,
        &Bba {
            bid: QuoteCurrency::new(99, 0),
            ask: QuoteCurrency::new(100, 0),
            timestamp_exchange_ns: 1.into(),
        },
    )
    .unwrap();

    // Open long 3 FTM @ 100 and rest 2 passive orders.
    pf.submit_market_order(
        ftm,
        MarketOrder::new(Side::Buy, BaseCurrency::new(3, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_limit_order(
        ftm,
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(90, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    pf.submit_limit_order(
        ftm,
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(110, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();

    // Switch to ReduceOnly: buying more fails, selling 1 succeeds.
    pf.set_market_status(ftm, MarketStatus::ReduceOnly).unwrap();
    assert_eq!(
        pf.submit_market_order(
            ftm,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap()
        )
        .unwrap_err(),
        PortfolioError::MarketReduceOnly(ftm)
    );
    pf.submit_market_order(
        ftm,
        MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pf.market(ftm).unwrap().position.quantity(),
        BaseCurrency::new(2, 0)
    );

    // Delist FTM at settlement price 120:
    // Cancels 2 resting orders, force-settles long 2 @ 120 (realized PnL = +40, zero fee).
    let settlement = pf.delist_market(ftm, QuoteCurrency::new(120, 0)).unwrap();
    assert_eq!(settlement.settled_quantity, BaseCurrency::new(2, 0));
    assert_eq!(settlement.realized_pnl, QuoteCurrency::new(40, 0));
    assert_eq!(settlement.cancelled_orders.len(), 2);
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(pf.market(ftm).unwrap().status, MarketStatus::Delisted);
    assert!(pf.market(ftm).unwrap().position.quantity().is_zero());
    assert!(pf.market(ftm).unwrap().active_limit_orders.is_empty());
    // 1000 - 1 (from selling 1 @ 99 vs entry 100) + 40 = 1039.
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(1039, 0));

    // Further orders on delisted market are rejected.
    assert_eq!(
        pf.submit_market_order(
            ftm,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap()
        )
        .unwrap_err(),
        PortfolioError::MarketDelisted(ftm)
    );
}

#[test]
fn single_exchange_and_portfolio_mass_cancel_operations() {
    // Single-symbol Exchange mass cancel by side and all.
    let mut ex = mock_exchange_linear();
    ex.update_state(&Bba {
        bid: QuoteCurrency::new(100, 0),
        ask: QuoteCurrency::new(101, 0),
        timestamp_exchange_ns: 1.into(),
    })
    .unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(95, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(96, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    ex.submit_limit_order(
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(105, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();

    let buy_cancels = ex.cancel_limit_orders_by_side(Side::Buy).unwrap();
    assert_eq!(buy_cancels.cancelled_count(), 2);
    assert_eq!(ex.account().active_limit_orders().len(), 1);

    let all_cancels = ex.cancel_all_limit_orders().unwrap();
    assert_eq!(all_cancels.cancelled_count(), 1);
    assert!(ex.account().active_limit_orders().is_empty());
    assert!(ex.account().order_margin().is_zero());

    // PortfolioExchange mass cancel across markets.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![
            make_linear_market_cfg("BTCUSDT", 2),
            make_linear_market_cfg("ETHUSDT", 2),
        ],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    let btc = MarketId::new(0);
    let eth = MarketId::new(1);
    for m in [btc, eth] {
        pf.update_market(
            m,
            &Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into(),
            },
        )
        .unwrap();
        pf.submit_limit_order(
            m,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(95, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let cancelled = pf.cancel_all_across_markets();
    assert_eq!(cancelled.len(), 2);
    assert_eq!(cancelled[0].0, btc);
    assert_eq!(cancelled[1].0, eth);
    assert!(pf.total_required_collateral().is_zero());
}

type LinearPortfolio = PortfolioExchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>;

fn bba(bid: i64, ask: i64, ts: i64) -> Bba<i64, DECIMALS> {
    Bba {
        bid: QuoteCurrency::new(bid, 0),
        ask: QuoteCurrency::new(ask, 0),
        timestamp_exchange_ns: ts.into(),
    }
}

/// Wallet 100 at 50x (IM 2%, MM 1%): long 10 `AAA` @ 100 and short 10 `BBB` @ 100,
/// then `BBB` falls to 79/80 (short uPnL +200) and `AAA` to `aaa_bid`/`aaa_bid + 1`.
fn hedged_linear_portfolio(aaa_bid: i64) -> (LinearPortfolio, MarketId, MarketId) {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(100, 0),
        vec![
            make_linear_market_cfg("AAA", 50),
            make_linear_market_cfg("BBB", 50),
        ],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    pf.enable_ledger(64);
    let (a, b) = (MarketId::new(0), MarketId::new(1));
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.update_market(b, &bba(100, 101, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_market_order(
        b,
        MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    // Move the winner first so no intermediate state is below maintenance margin.
    pf.update_market(b, &bba(79, 80, 2)).unwrap();
    pf.update_market(a, &bba(aaa_bid, aaa_bid + 1, 2)).unwrap();
    (pf, a, b)
}

#[test]
fn portfolio_closing_losing_leg_with_profitable_other_leg_is_not_bankrupt() {
    // AAA uPnL = -150, BBB uPnL = +200, total equity = 150 (healthy).
    let (mut pf, a, b) = hedged_linear_portfolio(85);
    assert_eq!(pf.total_equity(), QuoteCurrency::new(150, 0));

    let closed = pf
        .submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0)).unwrap(),
        )
        .unwrap();
    // The shared wallet carries the realized loss while BBB's profit covers it.
    assert_eq!(closed.solvency, Solvency::Solvent);
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(-50, 0));
    assert!(pf.balances().bad_debt().is_zero());
    assert_eq!(pf.total_equity(), QuoteCurrency::new(150, 0));

    let closed = pf
        .submit_market_order(
            b,
            MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(closed.solvency, Solvency::Solvent);
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(150, 0));
    assert!(pf.balances().bad_debt().is_zero());
    assert!(pf.ledger().unwrap().reconciles_to(pf.balances().equity()));
}

#[test]
fn portfolio_funding_on_losing_leg_with_profitable_other_leg_books_no_bad_debt() {
    let (mut pf, a, b) = hedged_linear_portfolio(99);
    // Long 10 @ mark 100 pays 15% funding = 150 > wallet 100.
    let rate = FundingRate::from(Decimal::try_from_scaled(15, 2).expect(EXPECT_DECIMAL));
    let funding = pf
        .apply_funding(a, rate, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(funding.cashflow, QuoteCurrency::new(-150, 0));
    assert_eq!(funding.solvency, Solvency::Solvent);
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(-50, 0));
    assert_eq!(
        pf.balances().cumulative_funding(),
        QuoteCurrency::new(-150, 0)
    );
    assert!(pf.balances().bad_debt().is_zero());

    for (m, side) in [(a, Side::Sell), (b, Side::Buy)] {
        pf.submit_market_order(m, MarketOrder::new(side, BaseCurrency::new(10, 0)).unwrap())
            .unwrap();
    }
    // -50 wallet - 10 (AAA sold @ 99) + 200 (BBB bought back @ 80).
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(140, 0));
    assert!(pf.balances().bad_debt().is_zero());
    assert!(pf.ledger().unwrap().reconciles_to(pf.balances().equity()));
}

#[test]
fn portfolio_delist_of_losing_leg_reports_actual_wallet_change() {
    let (mut pf, a, _) = hedged_linear_portfolio(99);
    let settlement = pf.delist_market(a, QuoteCurrency::new(85, 0)).unwrap();
    assert_eq!(settlement.realized_pnl, QuoteCurrency::new(-150, 0));
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(-50, 0));
    assert!(pf.balances().bad_debt().is_zero());
}

#[test]
fn portfolio_real_insolvency_still_books_bad_debt() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(100, 0),
        vec![make_linear_market_cfg("AAA", 50)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    pf.enable_ledger(16);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    // Gap down: closing at 80 loses 200 against a wallet of 100.
    assert_eq!(
        pf.update_market(a, &bba(80, 81, 2)),
        Err(RiskError::Liquidate)
    );
    assert!(pf.market(a).unwrap().position.quantity().is_zero());
    assert!(pf.balances().equity().is_zero());
    assert_eq!(pf.balances().bad_debt(), QuoteCurrency::new(100, 0));
    let ledger = pf.ledger().unwrap();
    assert!(ledger.reconciles_to(pf.balances().equity()));
    assert_eq!(
        ledger.entries().last().unwrap().kind(),
        LedgerEntryKind::BadDebtAbsorption
    );

    // Closing the last leg at a loss beyond the wallet also books bad debt.
    let mut pf = LinearPortfolio::new(
        PortfolioConfig::new(
            QuoteCurrency::new(100, 0),
            vec![make_linear_market_cfg("AAA", 50)],
        )
        .unwrap(),
    );
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap(),
    )
    .unwrap();
    let settlement = pf.delist_market(a, QuoteCurrency::new(80, 0)).unwrap();
    assert_eq!(settlement.realized_pnl, QuoteCurrency::new(-200, 0));
    assert_eq!(settlement.solvency, Solvency::Bankrupt);
    assert!(pf.balances().equity().is_zero());
    assert_eq!(pf.balances().bad_debt(), QuoteCurrency::new(100, 0));
}

/// Wallet 100 at 10x (IM 10%, MM 5%): long 4 `BTC` @ 100, short 4 `ETH` @ 100, `BTC`
/// rallies to 140 (+160), then `ETH` gaps to 163 (-252): equity 8 < MM -> liquidation.
fn liquidate_hedged_portfolio(
    tickers: [&str; 2],
) -> Balances<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>> {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(100, 0),
        tickers
            .iter()
            .map(|t| make_linear_market_cfg(t, 10))
            .collect(),
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let btc = pf.market_id_by_ticker("BTC").unwrap();
    let eth = pf.market_id_by_ticker("ETH").unwrap();
    pf.update_market(btc, &bba(99, 100, 1)).unwrap();
    pf.update_market(eth, &bba(100, 101, 1)).unwrap();
    pf.submit_market_order(
        btc,
        MarketOrder::new(Side::Buy, BaseCurrency::new(4, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_market_order(
        eth,
        MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap(),
    )
    .unwrap();
    pf.update_market(btc, &bba(140, 141, 2)).unwrap();
    assert_eq!(
        pf.update_market(eth, &bba(162, 163, 2)),
        Err(RiskError::Liquidate)
    );
    assert!(pf.market(btc).unwrap().position.quantity().is_zero());
    assert!(pf.market(eth).unwrap().position.quantity().is_zero());
    pf.balances().clone()
}

#[test]
fn portfolio_liquidation_outcome_is_independent_of_market_registration_order() {
    let btc_first = liquidate_hedged_portfolio(["BTC", "ETH"]);
    let eth_first = liquidate_hedged_portfolio(["ETH", "BTC"]);
    assert_eq!(btc_first, eth_first);
    // 100 + 160 - 252: the portfolio was still solvent, so no bad debt.
    assert_eq!(btc_first.equity(), QuoteCurrency::new(8, 0));
    assert!(btc_first.bad_debt().is_zero());
}

#[test]
fn inverse_portfolio_closing_losing_leg_with_profitable_other_leg_is_not_bankrupt() {
    let make_cfg = |ticker: &str| {
        let spec = ContractSpecification::new(
            Leverage::new(50).unwrap(),
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
            PriceFilter::default(),
            QuantityFilter::new(None, None, QuoteCurrency::new(1, 0))
                .expect(EXPECT_QUANTITY_FILTER),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::zero()),
        )
        .expect(EXPECT_CONTRACT_SPEC)
        .with_ticker(ticker);
        MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            spec,
            OrderRateLimits::default(),
        )
    };
    let cfg = PortfolioConfig::new(
        BaseCurrency::new(1, 0),
        vec![make_cfg("AAAUSD"), make_cfg("BBBUSD")],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    let (a, b) = (MarketId::new(0), MarketId::new(1));
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.update_market(b, &bba(100, 101, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, QuoteCurrency::new(500, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_market_order(
        b,
        MarketOrder::new(Side::Sell, QuoteCurrency::new(1000, 0)).unwrap(),
    )
    .unwrap();
    pf.update_market(b, &bba(79, 80, 2)).unwrap();
    pf.update_market(a, &bba(80, 81, 2)).unwrap();

    // Long 500 USD 100 -> 80 loses 1.25 BTC > wallet 1 BTC; short 1000 USD 100 -> 80 earns 2.5 BTC.
    let closed = pf
        .submit_market_order(
            a,
            MarketOrder::new(Side::Sell, QuoteCurrency::new(500, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(closed.solvency, Solvency::Solvent);
    assert_eq!(pf.balances().equity(), BaseCurrency::new(-25, 2));
    assert!(pf.balances().bad_debt().is_zero());
    pf.submit_market_order(
        b,
        MarketOrder::new(Side::Buy, QuoteCurrency::new(1000, 0)).unwrap(),
    )
    .unwrap();
    assert_eq!(pf.balances().equity(), BaseCurrency::new(225, 2));
    assert!(pf.balances().bad_debt().is_zero());
}
