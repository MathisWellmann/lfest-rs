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
    assert_eq!(res, Err(RiskError::Liquidate.into()));
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

    // Switch to ReduceOnly: the resting buy (it would grow the long) is cancelled, the
    // reducing sell stays; buying more fails, selling 1 succeeds.
    let cancelled = pf.set_market_status(ftm, MarketStatus::ReduceOnly).unwrap();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0].side(), Side::Buy);
    assert_eq!(pf.market(ftm).unwrap().active_limit_orders.len(), 1);
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
    // Cancels the resting sell, force-settles long 2 @ 120 (realized PnL = +40, zero fee).
    let settlement = pf.delist_market(ftm, QuoteCurrency::new(120, 0)).unwrap();
    assert_eq!(settlement.settled_quantity, BaseCurrency::new(2, 0));
    assert_eq!(settlement.realized_pnl, QuoteCurrency::new(40, 0));
    assert_eq!(settlement.cancelled_orders.len(), 1);
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
        Err(RiskError::Liquidate.into())
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
        Err(RiskError::Liquidate.into())
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

#[test]
fn portfolio_set_market_status_rejects_delisted() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![make_linear_market_cfg("AAA", 2)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pf.set_market_status(a, MarketStatus::Delisted),
        Err(PortfolioError::UseDelistMarket(a))
    );
    // The position is untouched and can still be closed.
    assert_eq!(pf.market(a).unwrap().status, MarketStatus::Active);
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    assert!(pf.market(a).unwrap().position.quantity().is_zero());
    assert_eq!(
        pf.set_market_status(MarketId::new(1), MarketStatus::ReduceOnly),
        Err(PortfolioError::UnknownMarket(MarketId::new(1)))
    );
}

fn rest_limit(pf: &mut LinearPortfolio, m: MarketId, side: Side, price: i64, qty: i64) -> OrderId {
    pf.submit_limit_order(
        m,
        LimitOrder::new(
            side,
            QuoteCurrency::new(price, 0),
            BaseCurrency::new(qty, 0),
        )
        .unwrap(),
    )
    .unwrap()
    .id()
}

#[test]
fn portfolio_reduce_only_status_cancels_risk_increasing_resting_orders() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(10_000, 0),
        vec![
            make_linear_market_cfg("AAA", 2),
            make_linear_market_cfg("BBB", 2),
        ],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let (a, b) = (MarketId::new(0), MarketId::new(1));
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.update_market(b, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    let grow = rest_limit(&mut pf, a, Side::Buy, 90, 1);
    let flip = rest_limit(&mut pf, a, Side::Sell, 110, 3);
    let reduce = rest_limit(&mut pf, a, Side::Sell, 111, 2);
    // Flat market: every order would open a position.
    rest_limit(&mut pf, b, Side::Buy, 90, 1);
    rest_limit(&mut pf, b, Side::Sell, 110, 1);

    let cancelled = pf.set_market_status(a, MarketStatus::ReduceOnly).unwrap();
    let mut ids: Vec<OrderId> = cancelled.iter().map(|o| o.id()).collect();
    ids.sort();
    assert_eq!(ids, vec![grow, flip]);
    let remaining = &pf.market(a).unwrap().active_limit_orders;
    assert_eq!(remaining.len(), 1);
    assert!(remaining.get_by_id(reduce, Side::Sell).is_some());

    assert_eq!(
        pf.set_market_status(b, MarketStatus::ReduceOnly)
            .unwrap()
            .len(),
        2
    );
    assert!(pf.market(b).unwrap().active_limit_orders.is_empty());
    // Switching back to active cancels nothing.
    assert!(
        pf.set_market_status(a, MarketStatus::Active)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn portfolio_reduce_only_market_fills_never_increase_the_position() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(10_000, 0),
        vec![make_linear_market_cfg("AAA", 2)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    // Each sell alone fits within the long, so both survive the status switch.
    rest_limit(&mut pf, a, Side::Sell, 110, 2);
    let second = rest_limit(&mut pf, a, Side::Sell, 111, 2);
    assert!(
        pf.set_market_status(a, MarketStatus::ReduceOnly)
            .unwrap()
            .is_empty()
    );

    // A taker buy sweeping both: the first closes the long, the second must not open a short.
    let events = pf
        .update_market(
            a,
            &Trade {
                price: QuoteCurrency::new(112, 0),
                quantity: BaseCurrency::new(10, 0),
                side: Side::Buy,
                timestamp_exchange_ns: 2.into(),
            },
        )
        .unwrap()
        .clone();
    assert!(pf.market(a).unwrap().position.quantity().is_zero());
    assert!(pf.market(a).unwrap().active_limit_orders.is_empty());
    assert!(events.iter().any(|e| matches!(
        &e.event,
        LimitOrderEvent::ForcedCancel(o) if o.id() == second
    )));
}

#[test]
fn portfolio_update_market_rejects_unknown_market() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![make_linear_market_cfg("AAA", 2)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let unknown = MarketId::new(1);
    assert_eq!(
        pf.update_market(unknown, &bba(99, 100, 1)),
        Err(PortfolioError::UnknownMarket(unknown))
    );
}

#[test]
fn portfolio_rejects_duplicate_tickers() {
    assert_eq!(
        PortfolioConfig::new(
            QuoteCurrency::<i64, DECIMALS>::new(1000, 0),
            vec![
                make_linear_market_cfg("AAA", 2),
                make_linear_market_cfg("AAA", 3),
            ],
        )
        .unwrap_err(),
        ConfigError::DuplicateMarketTicker
    );

    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![
            make_linear_market_cfg("AAA", 2),
            // Untickered markets are only addressed by id, so they may repeat.
            make_linear_market_cfg("", 2),
            make_linear_market_cfg("", 2),
        ],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    assert_eq!(
        pf.add_market(make_linear_market_cfg("AAA", 5)),
        Err(PortfolioError::DuplicateTicker("AAA".to_string()))
    );
    assert_eq!(pf.market_count(), 3);
    assert_eq!(pf.market_id_by_ticker("AAA"), Some(MarketId::new(0)));
    assert_eq!(
        pf.add_market(make_linear_market_cfg("", 2)),
        Ok(MarketId::new(3))
    );
}

#[test]
fn portfolio_supports_one_market_per_market_id() {
    let market = make_linear_market_cfg("", 2);
    let mut markets = vec![market.clone(); MarketId::COUNT + 1];
    assert_eq!(
        PortfolioConfig::new(QuoteCurrency::new(1000, 0), markets.clone()).unwrap_err(),
        ConfigError::TooManyMarkets
    );

    // `MarketId(u16::MAX)` is a valid id, so exactly `MarketId::COUNT` markets fit.
    markets.pop();
    let mut pf =
        LinearPortfolio::new(PortfolioConfig::new(QuoteCurrency::new(1000, 0), markets).unwrap());
    assert_eq!(pf.market_count(), MarketId::COUNT);
    assert!(pf.market(MarketId::new(u16::MAX)).is_some());
    assert_eq!(
        pf.add_market(market),
        Err(PortfolioError::MaxMarketsReached(MarketId::COUNT))
    );
}

fn cloned_two_market_portfolio() -> (LinearPortfolio, MarketId) {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(10_000, 0),
        vec![make_linear_market_cfg("AAA", 2)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    // Dynamic registration must leave the buffers sized for both markets as well.
    let b = pf.add_market(make_linear_market_cfg("BBB", 2)).unwrap();
    pf.update_market(b, &bba(99, 100, 1)).unwrap();
    (pf.clone(), b)
}

#[test]
fn cloned_portfolio_reports_a_fill() {
    // A fill pushes into `order_events`, which a derived `Clone` left without spare
    // capacity.
    let (mut pf, b) = cloned_two_market_portfolio();
    rest_limit(&mut pf, b, Side::Buy, 98, 1);
    let events = pf
        .update_market(
            b,
            &Trade {
                price: QuoteCurrency::new(97, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
                timestamp_exchange_ns: 2.into(),
            },
        )
        .unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(matches!(events[0].event, LimitOrderEvent::Fill(_)));
    assert_eq!(
        pf.market(b).unwrap().position.quantity(),
        BaseCurrency::new(1, 0)
    );
}

#[test]
fn cloned_portfolio_reports_a_triggered_stop() {
    // Election pushes into the elected-stop scratch buffer, which a derived `Clone` left
    // without spare capacity.
    let (mut pf, b) = cloned_two_market_portfolio();
    pf.submit_market_order(
        b,
        MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    pf.submit_stop_order(
        b,
        StopOrder::stop_market(
            Side::Sell,
            QuoteCurrency::new(95, 0),
            BaseCurrency::new(1, 0),
            NoUserOrderId,
        )
        .unwrap()
        .with_trigger_price_type(TriggerPriceType::Last),
    )
    .unwrap();

    let mut cloned = pf.clone();
    cloned
        .update_market(
            b,
            &Trade {
                price: QuoteCurrency::new(94, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
                timestamp_exchange_ns: 2.into(),
            },
        )
        .unwrap();

    assert!(cloned.pending_stops(b).expect("market exists").is_empty());
    assert!(cloned.market(b).unwrap().position.quantity().is_zero());
    let events = cloned.stop_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].market_id, b);
    assert!(matches!(events[0].event, StopOrderEvent::Filled { .. }));
}

#[test]
fn cloned_portfolio_reports_a_forced_cancel() {
    // Shrinking the position below a resting reduce-only order force-cancels it through
    // `forced_cancel_scratch`, which a derived `Clone` left without spare capacity.
    let (mut pf, b) = cloned_two_market_portfolio();
    pf.submit_market_order(
        b,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    let reduce = pf
        .submit_limit_order(
            b,
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(110, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap()
            .with_reduce_only(true),
        )
        .unwrap()
        .id();

    let mut cloned = pf.clone();
    let settlement = cloned
        .submit_market_order(
            b,
            MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();

    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0].1.id(), reduce);
    assert!(cloned.market(b).unwrap().active_limit_orders.is_empty());
}

fn make_linear_market_cfg_with_fees(
    ticker: &str,
    lev: u8,
    maker_fee_bps: i64,
    taker_fee_bps: i64,
) -> MarketConfig<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>> {
    let spec = ContractSpecification::new(
        Leverage::new(lev).unwrap(),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::try_from_scaled(maker_fee_bps, 4).expect(EXPECT_DECIMAL)),
        Fee::from(Decimal::try_from_scaled(taker_fee_bps, 4).expect(EXPECT_DECIMAL)),
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
fn portfolio_market_order_same_direction_enforces_exact_margin_and_fee_boundary() {
    // 5x (IM=20%), taker fee = 10% (1000 bps) so init_margin + fee = 0.2 + 0.1 = 0.3 of notional.
    // Buying 1 @ 100 requires 20 IM + 10 fee = 30.
    let build = |wallet_scaled: i64| {
        let cfg = PortfolioConfig::new(
            QuoteCurrency::new(wallet_scaled, 5),
            vec![make_linear_market_cfg_with_fees("AAA", 5, 0, 1000)],
        )
        .unwrap();
        let mut pf = LinearPortfolio::new(cfg);
        pf.update_market(MarketId::new(0), &bba(99, 100, 1))
            .unwrap();
        pf
    };

    // Exact boundary: available == init_margin + fee (30.00000) is accepted.
    // After buying @ ask 100 while bid is 99, the position immediately has -1 uPnL
    // (marked at bid), so total_equity = 19 < IM 20 -> InitialMarginDeficit.
    let mut exact = build(3_000_000);
    let settlement = exact
        .submit_market_order(
            MarketId::new(0),
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(settlement.solvency, Solvency::InitialMarginDeficit);
    assert_eq!(
        exact.market(MarketId::new(0)).unwrap().position.quantity(),
        BaseCurrency::new(1, 0)
    );

    // One unit of precision below (29.99999) is rejected.
    let mut short = build(2_999_999);
    assert!(matches!(
        short.submit_market_order(
            MarketId::new(0),
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        ),
        Err(PortfolioError::SubmitMarketOrder(
            SubmitMarketOrderError::NotEnoughAvailableBalance(_)
        ))
    ));
}

#[test]
fn portfolio_market_order_flip_accounts_for_realized_pnl_fees_other_market_and_boundary() {
    // Market A: 5x (IM=20%), taker fee 10% (1000 bps).
    // Market B: 5x (IM=20%), 0 fee.
    // Setup:
    //   Long 2 A @ ask 100 (entry=100, notional=200, IM=40, fee=20).
    //   Long 1 B @ ask 100 (entry=100, notional=100, IM=20, fee=0).
    //   Move A to bid 120 / ask 121 (A uPnL = +40).
    //   Move B to bid 110 / ask 111 (B uPnL = +10).
    // Now submit Sell 5 A @ bid 120 -> closes long 2 (realized_pnl = +40) and opens short 3 @ 120
    // (flip_notional = 360, flip IM = 72). Total sell notional = 600, fee = 60.
    // Wallet after initial buys = W - 20.
    // post_equity = (W - 20) + 40 (realized) - 60 (fee) + 10 (other_upnl B) = W - 30.
    // post_req    = 20 (other_req B) + 72 (flip IM) = 92.
    // Exact boundary: W - 30 == 92 => W = 122.
    let setup = |wallet_scaled: i64| {
        let cfg = PortfolioConfig::new(
            QuoteCurrency::new(wallet_scaled, 5),
            vec![
                make_linear_market_cfg_with_fees("AAA", 5, 0, 1000),
                make_linear_market_cfg_with_fees("BBB", 5, 0, 0),
            ],
        )
        .unwrap();
        let mut pf = LinearPortfolio::new(cfg);
        let (a, b) = (MarketId::new(0), MarketId::new(1));
        pf.update_market(a, &bba(99, 100, 1)).unwrap();
        pf.update_market(b, &bba(99, 100, 1)).unwrap();
        pf.submit_market_order(
            a,
            MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
        )
        .unwrap();
        pf.submit_market_order(
            b,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();
        pf.update_market(a, &bba(120, 121, 2)).unwrap();
        pf.update_market(b, &bba(110, 111, 2)).unwrap();
        (pf, a)
    };

    // Exact boundary W = 122.00000: flip is admitted; the new short 3 @ 120 marks at ask 121
    // (uPnL = -3), leaving margin_excess = -3 -> InitialMarginDeficit.
    let (mut pf_ok, a) = setup(12_200_000);
    let settlement = pf_ok
        .submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(settlement.solvency, Solvency::InitialMarginDeficit);
    assert!(settlement.forced_cancels.is_empty());
    assert_eq!(
        pf_ok.market(a).unwrap().position.quantity(),
        BaseCurrency::new(-3, 0)
    );
    assert_eq!(pf_ok.margin_excess(), QuoteCurrency::new(-3, 0));

    // One unit below (W = 121.99999): flip is rejected without mutating positions.
    let (mut pf_err, a) = setup(12_199_999);
    assert!(matches!(
        pf_err.submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0)).unwrap(),
        ),
        Err(PortfolioError::SubmitMarketOrder(
            SubmitMarketOrderError::NotEnoughAvailableBalance(_)
        ))
    ));
    assert_eq!(
        pf_err.market(a).unwrap().position.quantity(),
        BaseCurrency::new(2, 0)
    );
}

#[test]
fn portfolio_market_order_flip_rejects_when_resting_orders_lose_position_offset() {
    // Long 2 A @ 100 (IM=20%, 0 fees), resting sell limit 2 A @ 110 (fully offset while long 2,
    // so order_margin = 0). Flipping with Sell 3 A @ bid 100 opens Short 1 A @ 100 (pos IM = 20),
    // and the resting sell 2 @ 110 is no longer offset by a long, so post_order_margin = 220 * 0.2 = 44.
    // Total post_req = 20 + 44 = 64. With wallet = 50, equity after flip = 50 < 64 -> must reject.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(50, 0),
        vec![make_linear_market_cfg_with_fees("AAA", 5, 0, 0)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
    )
    .unwrap();
    // Move quotes so the long has zero unrealized loss at bid 100 / ask 101.
    pf.update_market(a, &bba(100, 101, 2)).unwrap();
    pf.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(110, 0),
            BaseCurrency::new(2, 0),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(pf.total_required_collateral(), QuoteCurrency::new(44, 0));

    assert!(matches!(
        pf.submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap(),
        ),
        Err(PortfolioError::SubmitMarketOrder(
            SubmitMarketOrderError::NotEnoughAvailableBalance(_)
        ))
    ));
    assert_eq!(
        pf.market(a).unwrap().position.quantity(),
        BaseCurrency::new(2, 0)
    );
    assert_eq!(pf.market(a).unwrap().active_limit_orders.len(), 1);

    // Confirm a portfolio with wallet = 64 (exact post_req = 20 flip IM + 44 resting ask IM)
    // is admitted.
    let cfg_ok = PortfolioConfig::new(
        QuoteCurrency::new(64, 0),
        vec![make_linear_market_cfg_with_fees("AAA", 5, 0, 0)],
    )
    .unwrap();
    let mut pf_ok = LinearPortfolio::new(cfg_ok);
    pf_ok.update_market(a, &bba(99, 100, 1)).unwrap();
    pf_ok
        .submit_market_order(
            a,
            MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
        )
        .unwrap();
    pf_ok.update_market(a, &bba(100, 101, 2)).unwrap();
    pf_ok
        .submit_limit_order(
            a,
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(110, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        )
        .unwrap();
    let settlement = pf_ok
        .submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        pf_ok.market(a).unwrap().position.quantity(),
        BaseCurrency::new(-1, 0)
    );
    // Spread marks short 1 @ 100 against ask 101 (uPnL = -1), so margin_excess = -1 forces
    // cancellation of the resting ask on reconciliation.
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
}

#[test]
fn portfolio_market_order_flip_handles_realized_loss_and_negative_other_upnl() {
    // Test with negative realized_pnl and negative other_upnl to catch sign flips on both terms.
    // Market A: 5x (IM=20%), 0 fee. Market B: 5x (IM=20%), 0 fee.
    // Long 2 A @ 100 (IM=40), Long 1 B @ 100 (IM=20).
    // A drops to 90/91 (uPnL = -20), B drops to 80/81 (uPnL = -20).
    // Sell 3 A @ bid 90: closes long 2 (realized_pnl = -20), opens short 1 @ 90 (flip IM = 18).
    // post_equity = W - 20 (realized) - 20 (other_upnl B) = W - 40.
    // post_req    = 20 (other_req B) + 18 (flip IM) = 38.
    // Exact boundary: W = 78.
    let setup = |wallet_scaled: i64| {
        let cfg = PortfolioConfig::new(
            QuoteCurrency::new(wallet_scaled, 5),
            vec![
                make_linear_market_cfg("AAA", 5),
                make_linear_market_cfg("BBB", 5),
            ],
        )
        .unwrap();
        let mut pf = LinearPortfolio::new(cfg);
        let (a, b) = (MarketId::new(0), MarketId::new(1));
        pf.update_market(a, &bba(99, 100, 1)).unwrap();
        pf.update_market(b, &bba(99, 100, 1)).unwrap();
        pf.submit_market_order(
            a,
            MarketOrder::new(Side::Buy, BaseCurrency::new(2, 0)).unwrap(),
        )
        .unwrap();
        pf.submit_market_order(
            b,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();
        pf.update_market(a, &bba(90, 91, 2)).unwrap();
        pf.update_market(b, &bba(80, 81, 2)).unwrap();
        (pf, a)
    };

    let (mut pf_ok, a) = setup(7_800_000);
    let settlement = pf_ok
        .submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap(),
        )
        .unwrap();
    // New short 1 @ 90 marks at ask 91 (-1 spread uPnL), so margin_excess = -1.
    assert_eq!(settlement.solvency, Solvency::InitialMarginDeficit);
    assert_eq!(pf_ok.margin_excess(), QuoteCurrency::new(-1, 0));

    let (mut pf_err, a) = setup(7_799_999);
    assert!(matches!(
        pf_err.submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(3, 0)).unwrap(),
        ),
        Err(PortfolioError::SubmitMarketOrder(
            SubmitMarketOrderError::NotEnoughAvailableBalance(_)
        ))
    ));
}

#[test]
fn portfolio_limit_order_check_enforces_exact_margin_delta_and_maker_fee_reserve() {
    // 5x (IM=20%), maker fee = 5% (500 bps).
    // First rest Buy 1 @ 80: order_margin = 16, reserved_maker_fee = 4 -> total req = 20.
    // Second rest Buy 2 @ 80: new order_margin = 48 (delta = 32), extra_fee_reserve = 8 -> extra_req = 40.
    // Total required = 60. Test exact boundary W = 60 vs W = 59.99999.
    let setup = |wallet_scaled: i64| {
        let cfg = PortfolioConfig::new(
            QuoteCurrency::new(wallet_scaled, 5),
            vec![make_linear_market_cfg_with_fees("AAA", 5, 500, 0)],
        )
        .unwrap();
        let mut pf = LinearPortfolio::new(cfg);
        let a = MarketId::new(0);
        pf.update_market(a, &bba(99, 100, 1)).unwrap();
        pf.submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(80, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(pf.total_required_collateral(), QuoteCurrency::new(20, 0));
        (pf, a)
    };

    let (mut pf_ok, a) = setup(6_000_000);
    pf_ok
        .submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(80, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(pf_ok.total_required_collateral(), QuoteCurrency::new(60, 0));
    assert_eq!(pf_ok.margin_excess(), QuoteCurrency::zero());

    let (mut pf_err, a) = setup(5_999_999);
    assert!(matches!(
        pf_err.submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(80, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        ),
        Err(PortfolioError::SubmitLimitOrder(
            SubmitLimitOrderError::NotEnoughAvailableBalance(_)
        ))
    ));
}
