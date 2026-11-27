//! Integration tests for multi-symbol `PortfolioExchange`, cross-margin netting,
//! `MarketId(u16)` indexing, market lifecycle (`ReduceOnly`, `delist_market`), and
//! mass-cancel (`cancel_all_limit_orders`, `cancel_limit_orders_by_side`).

use std::num::NonZeroU16;

use const_decimal::Decimal;

use super::bba;
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

#[test]
fn portfolio_reserved_maker_fees_and_required_collateral_sum_bids_and_asks() {
    // 5x (IM=20%), maker fee = 5% (500 bps).
    // Buy 2 @ 80 (notional 160), Sell 1 @ 120 (notional 120).
    // order_margin = max(160, 120) * 0.20 = 32.
    // reserved_maker_fees = (160 + 120) * 0.05 = 14.
    // required_collateral = 0 + 32 + 14 = 46.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(100, 0),
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
            BaseCurrency::new(2, 0),
        )
        .unwrap(),
    )
    .unwrap();
    pf.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(120, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(pf.total_required_collateral(), QuoteCurrency::new(46, 0));
    assert_eq!(pf.margin_excess(), QuoteCurrency::new(54, 0));
}

#[test]
fn portfolio_margin_call_selects_largest_marginal_collateral_contributor_across_sides_and_markets()
{
    // Two markets, both 5x (IM=20%).
    // Market A (maker fee 1% = 100 bps): flat, resting Buy 2 @ 80 (notional 160),
    //   Sell 1 @ 110 (notional 110), and Sell 1 @ 120 (notional 120).
    //   asks_notional = 230 > bids_notional = 160 -> order_margin = 230 * 0.20 = 46.
    //   Marginal deltas on A:
    //   - Buy 2 @ 80 (notional 160): removing leaves max(0, 230)*0.2 = 46 -> delta = 0 + 1.6 = 1.6.
    //   - Sell 1 @ 110 (notional 110): removing leaves max(160, 120)*0.2 = 32 -> delta = 14 + 1.1 = 15.1.
    //   - Sell 1 @ 120 (notional 120): removing leaves max(160, 110)*0.2 = 32 -> delta = 14 + 1.2 = 15.2.
    //   Note: with `/` instead of `-` in asks_notional - order.notional(), 230 / 110 = 2.09 and
    //   230 / 120 = 1.91 both drop below bids_notional = 160, giving the same delta = 14.
    //   Add a third case with no bids on A (bids = 0) and asks 110 + 120 = 230, where removing 120
    //   leaves 110 (delta = 24 + 1.2 = 25.2) vs B's Buy 1.4 @ 90 (notional 126, delta = 25.2 ->
    //   use Buy 1.5 @ 90, delta = 27.0): under `/` A's delta would become (230 - 1.91)*0.2 + 1.2 = 46.8 > 27.0!
    // Market B (maker fee 0%): Long 1 @ 100 (IM=20) + resting Buy qty @ 90.
    let run_case = |b_bid_qty_scaled: i64| {
        let cfg = PortfolioConfig::new(
            QuoteCurrency::new(88, 0),
            vec![
                make_linear_market_cfg_with_fees("AAA", 5, 100, 0),
                make_linear_market_cfg_with_fees("BBB", 5, 0, 0),
            ],
        )
        .unwrap();
        let mut pf = LinearPortfolio::new(cfg);
        let (a, b) = (MarketId::new(0), MarketId::new(1));
        pf.update_market(a, &bba(99, 100, 1)).unwrap();
        pf.update_market(b, &bba(99, 100, 1)).unwrap();
        // Open Long 1 B @ 100, then mark at 100/101 so uPnL is 0.
        pf.submit_market_order(
            b,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();
        pf.update_market(b, &bba(100, 101, 2)).unwrap();

        let _a_buy = pf
            .submit_limit_order(
                a,
                LimitOrder::new(
                    Side::Buy,
                    QuoteCurrency::new(80, 0),
                    BaseCurrency::new(2, 0),
                )
                .unwrap(),
            )
            .unwrap()
            .id();
        let _a_sell_110 = pf
            .submit_limit_order(
                a,
                LimitOrder::new(
                    Side::Sell,
                    QuoteCurrency::new(110, 0),
                    BaseCurrency::new(1, 0),
                )
                .unwrap(),
            )
            .unwrap()
            .id();
        let a_sell_120 = pf
            .submit_limit_order(
                a,
                LimitOrder::new(
                    Side::Sell,
                    QuoteCurrency::new(120, 0),
                    BaseCurrency::new(1, 0),
                )
                .unwrap(),
            )
            .unwrap()
            .id();
        let b_buy = pf
            .submit_limit_order(
                b,
                LimitOrder::new(
                    Side::Buy,
                    QuoteCurrency::new(90, 0),
                    BaseCurrency::new(b_bid_qty_scaled, 1),
                )
                .unwrap(),
            )
            .unwrap()
            .id();
        (pf, a, b, a_sell_120, b_buy)
    };

    // Case 1: B's Buy 1 @ 90 (delta 18.0) beats A's Sell 1 @ 120 (delta 15.2).
    let (mut pf1, _a, b, _a_sell_120, b_buy) = run_case(10);
    let rate = FundingRate::from(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL));
    let settle1 = pf1
        .apply_funding(b, rate, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(settle1.solvency, Solvency::Solvent);
    assert_eq!(settle1.forced_cancels.len(), 1);
    assert_eq!(settle1.forced_cancels[0].0, b);
    assert_eq!(settle1.forced_cancels[0].1.id(), b_buy);

    // Case 2: B's Buy 0.7 @ 90 (delta 12.6) loses to A's Sell 1 @ 120 (delta 15.2),
    // which also beats A's Sell 1 @ 110 (delta 15.1) via the maker-fee term and beats
    // A's Buy 2 @ 80 (notional 160, delta 1.6).
    let (mut pf2, a, b, a_sell_120, _b_buy) = run_case(7);
    let rate6 = FundingRate::from(Decimal::try_from_scaled(6, 2).expect(EXPECT_DECIMAL));
    let settle2 = pf2
        .apply_funding(b, rate6, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(settle2.solvency, Solvency::Solvent);
    assert_eq!(settle2.forced_cancels.len(), 1);
    assert_eq!(settle2.forced_cancels[0].0, a);
    assert_eq!(settle2.forced_cancels[0].1.id(), a_sell_120);

    // Case 3 (ask subtraction vs division): A has two asks (110 + 120 = 230, no bids) so each
    // ask's true delta is 120*0.2 = 24 (plus 1.2 fee = 25.2); B has a single bid of notional 150
    // (delta = 30.0). Replacing `-` with `/` on asks_notional makes A's delta ~46.8 > 30.0.
    let cfg3 = PortfolioConfig::new(
        QuoteCurrency::new(100, 0),
        vec![
            make_linear_market_cfg_with_fees("AAA", 5, 100, 0),
            make_linear_market_cfg_with_fees("BBB", 5, 0, 0),
        ],
    )
    .unwrap();
    let mut pf3 = LinearPortfolio::new(cfg3);
    let (a, b) = (MarketId::new(0), MarketId::new(1));
    pf3.update_market(a, &bba(99, 100, 1)).unwrap();
    pf3.update_market(b, &bba(99, 100, 1)).unwrap();
    pf3.submit_market_order(
        b,
        MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    pf3.update_market(b, &bba(100, 101, 2)).unwrap();
    pf3.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(110, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    pf3.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(120, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    let b_single = pf3
        .submit_limit_order(
            b,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(75, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        )
        .unwrap()
        .id();
    // Total req = 46 + 2.3 (A) + 20 + 30 (B) = 98.3, wallet = 100 -> debit 2.0 -> excess -0.3.
    let rate2 = FundingRate::from(Decimal::try_from_scaled(2, 2).expect(EXPECT_DECIMAL));
    let settle3 = pf3
        .apply_funding(b, rate2, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(settle3.solvency, Solvency::Solvent);
    assert_eq!(settle3.forced_cancels.len(), 1);
    assert_eq!(settle3.forced_cancels[0].0, b);
    assert_eq!(settle3.forced_cancels[0].1.id(), b_single);
}

#[test]
fn portfolio_margin_call_selects_dominant_bid_contributor_correctly() {
    // 5x (IM=20%), maker fee = 1% (100 bps). Long 1 @ 100 (IM=20, marked @ 100/101 -> uPnL 0).
    // Bids: Buy 1 @ 80 (notional 80) and Buy 1 @ 90 (notional 90) -> bids_notional = 170.
    // Asks: Sell 2 @ 120 (notional 240, offset by pos notional 100 -> asks - pos = 140 < 170).
    // Current order_margin = 170 * 0.2 = 34.
    // Marginal deltas:
    //   - Buy 1 @ 90: removing leaves max(80, 140)*0.2 = 28 -> delta = 6 + 0.9 = 6.9.
    //   - Buy 1 @ 80: removing leaves max(90, 140)*0.2 = 28 -> delta = 6 + 0.8 = 6.8.
    //   - Sell 2 @ 120: removing leaves max(170, -100)*0.2 = 34 -> delta = 0 + 2.4 = 2.4.
    // Total required = 20 (pos) + 34 (orders) + 4.1 (maker fees) = 58.1.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(59, 0),
        vec![make_linear_market_cfg_with_fees("AAA", 5, 100, 0)],
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
    pf.update_market(a, &bba(100, 101, 2)).unwrap();

    let _buy_80 = pf
        .submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(80, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap()
        .id();
    let buy_90 = pf
        .submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(90, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap()
        .id();
    let _sell_120 = pf
        .submit_limit_order(
            a,
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(120, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        )
        .unwrap()
        .id();

    // Debit 1.0 via 1% funding -> equity 58.0 < 58.1 (excess -0.1). Must cancel buy_90 (delta 6.9).
    let rate = FundingRate::from(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL));
    let settlement = pf
        .apply_funding(a, rate, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(settlement.solvency, Solvency::Solvent);
    assert_eq!(settlement.forced_cancels.len(), 1);
    assert_eq!(settlement.forced_cancels[0].1.id(), buy_90);

    // Bid subtraction vs division: A has two bids (80 + 90 = 170, no asks), so each bid's true
    // delta is 90*0.2 = 18.0 (plus 0.9 fee = 18.9); B has a single ask of notional 110 (delta 22.0).
    // Replacing `-` with `/` on bids_notional makes A's delta ~(170 - 1.89)*0.2 + 0.9 = 34.5 > 22.0.
    let cfg2 = PortfolioConfig::new(
        QuoteCurrency::new(80, 0),
        vec![
            make_linear_market_cfg_with_fees("AAA", 5, 100, 0),
            make_linear_market_cfg_with_fees("BBB", 5, 0, 0),
        ],
    )
    .unwrap();
    let mut pf2 = LinearPortfolio::new(cfg2);
    let (a, b) = (MarketId::new(0), MarketId::new(1));
    pf2.update_market(a, &bba(99, 100, 1)).unwrap();
    pf2.update_market(b, &bba(99, 100, 1)).unwrap();
    pf2.submit_market_order(
        b,
        MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    pf2.update_market(b, &bba(98, 99, 2)).unwrap();
    pf2.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(80, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    pf2.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(90, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    let b_ask = pf2
        .submit_limit_order(
            b,
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(110, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        )
        .unwrap()
        .id();
    // Total req = 34 + 1.7 (A) + 19.8 + 22 (B) = 77.5, wallet = 80 -> debit 3.0 -> excess -0.5.
    let rate_neg3 = FundingRate::from(Decimal::try_from_scaled(-3, 2).expect(EXPECT_DECIMAL));
    let settle2 = pf2
        .apply_funding(b, rate_neg3, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(settle2.solvency, Solvency::Solvent);
    assert_eq!(settle2.forced_cancels.len(), 1);
    assert_eq!(settle2.forced_cancels[0].0, b);
    assert_eq!(settle2.forced_cancels[0].1.id(), b_ask);
}

#[test]
fn portfolio_reconcile_margin_respects_exact_zero_and_maintenance_boundaries() {
    // 5x (IM=20%, MM=10%). Long 1 @ 100 (IM=20, MM=10), marked at 100/101 -> uPnL = 0.
    let setup = |wallet: i64| {
        let cfg = PortfolioConfig::new(
            QuoteCurrency::new(wallet, 0),
            vec![make_linear_market_cfg("AAA", 5)],
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
        pf.update_market(a, &bba(100, 101, 2)).unwrap();
        (pf, a)
    };

    // 1. Exact margin_excess == 0 with a resting order: must NOT force-cancel and stays Solvent.
    let (mut pf1, a) = setup(35);
    pf1.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(50, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();
    let rate5 = FundingRate::from(Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL));
    let s1 = pf1
        .apply_funding(a, rate5, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(s1.solvency, Solvency::Solvent);
    assert!(s1.forced_cancels.is_empty());
    assert_eq!(pf1.market(a).unwrap().active_limit_orders.len(), 1);
    assert_eq!(pf1.margin_excess(), QuoteCurrency::zero());

    // 2. Exact total_equity == total_maintenance_margin (10 == 10) with no resting orders:
    //    must NOT liquidate, and must report InitialMarginDeficit (10 < IM 20).
    let (mut pf2, a) = setup(20);
    let rate10 = FundingRate::from(Decimal::try_from_scaled(10, 2).expect(EXPECT_DECIMAL));
    let s2 = pf2
        .apply_funding(a, rate10, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(s2.solvency, Solvency::InitialMarginDeficit);
    assert_eq!(pf2.total_equity(), QuoteCurrency::new(10, 0));
    assert_eq!(pf2.total_maintenance_margin(), QuoteCurrency::new(10, 0));
    assert_eq!(
        pf2.market(a).unwrap().position.quantity(),
        BaseCurrency::new(1, 0)
    );

    // 3. Exact margin_excess == 0 with no resting orders reports Solvent (not InitialMarginDeficit).
    let (mut pf3, a) = setup(20);
    let s3 = pf3
        .apply_funding(
            a,
            FundingRate::from(Decimal::zero()),
            QuoteCurrency::new(100, 0),
        )
        .unwrap();
    assert_eq!(s3.solvency, Solvency::Solvent);
    assert_eq!(pf3.margin_excess(), QuoteCurrency::zero());

    // 4. One tick of funding below MM (total_equity < 10) liquidates.
    let rate_tiny = FundingRate::from(Decimal::try_from_scaled(1, 5).expect(EXPECT_DECIMAL));
    let s4 = pf2
        .apply_funding(a, rate_tiny, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(s4.solvency, Solvency::Liquidated);
    assert!(pf2.market(a).unwrap().position.quantity().is_zero());
}

#[test]
fn portfolio_update_market_at_exact_maintenance_margin_does_not_liquidate() {
    // 5x (IM=20%, MM=10%). Open Long 1 @ 100 with wallet 20, debit 1 via funding -> wallet = 19.
    // At bid = 90: equity = 19 + (90 - 100) = 9 == MM (90 * 0.1 = 9) -> survives.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(20, 0),
        vec![make_linear_market_cfg("AAA", 5)],
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
    pf.apply_funding(
        a,
        FundingRate::from(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL)),
        QuoteCurrency::new(100, 0),
    )
    .unwrap();
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(19, 0));

    pf.update_market(a, &bba(90, 91, 2)).unwrap();
    assert_eq!(pf.total_equity(), QuoteCurrency::new(9, 0));
    assert_eq!(pf.total_maintenance_margin(), QuoteCurrency::new(9, 0));
    assert_eq!(
        pf.market(a).unwrap().position.quantity(),
        BaseCurrency::new(1, 0)
    );

    // Drop bid by 1 tick below 90 -> liquidates.
    let err = pf.update_market(a, &bba(89, 90, 3)).unwrap_err();
    assert!(matches!(err, PortfolioError::Risk(RiskError::Liquidate)));
}

#[test]
fn portfolio_amend_limit_order_distinguishes_missing_ids_and_enforces_cboe_leaves_qty() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![make_linear_market_cfg("AAA", 5)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();

    // 1. Order id equal to next_order_id (0, never issued yet) -> OrderIdNotFound (not OrderNoLongerActive).
    let unseen_id = OrderId::from(0u64);
    assert!(matches!(
        pf.amend_limit_order(
            a,
            unseen_id,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(90, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        ),
        Err(PortfolioError::AmendLimitOrder(
            AmendLimitOrderError::OrderIdNotFound { order_id }
        )) if order_id == unseen_id
    ));

    // 2. Submit Buy 5 @ 95 (order id 0), partially fill 2 via a Sell trade @ 94 -> remaining 3.
    let orig = pf
        .submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(95, 0),
                BaseCurrency::new(5, 0),
            )
            .unwrap(),
        )
        .unwrap();
    pf.update_market(
        a,
        &Trade {
            price: QuoteCurrency::new(94, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 2.into(),
        },
    )
    .unwrap();
    assert_eq!(
        pf.market(a)
            .unwrap()
            .active_limit_orders
            .best_bid()
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(3, 0)
    );

    // 3. Amend total_qty from 5 -> 7: qty_delta = +2, new_leaves_qty = 3 + 2 = 5.
    //    `set_remaining_quantity(5)` updates the replacement order's size to 5.
    let amended = pf
        .amend_limit_order(
            a,
            orig.id(),
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(94, 0),
                BaseCurrency::new(7, 0),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(amended.total_quantity(), BaseCurrency::new(5, 0));
    assert_eq!(amended.remaining_quantity(), BaseCurrency::new(5, 0));

    // 4. Amending the already-replaced order id (< next_order_id) -> OrderNoLongerActive.
    assert!(matches!(
        pf.amend_limit_order(
            a,
            orig.id(),
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(94, 0),
                BaseCurrency::new(4, 0),
            )
            .unwrap(),
        ),
        Err(PortfolioError::AmendLimitOrder(
            AmendLimitOrderError::OrderNoLongerActive
        ))
    ));

    // 5. Post-only crossing amend is rejected with GoodTillCrossingRejectedOrder (not
    //    WouldExecuteImmediately) and leaves the amended order intact.
    assert!(matches!(
        pf.amend_limit_order(
            a,
            amended.id(),
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(105, 0),
                BaseCurrency::new(7, 0),
            )
            .unwrap()
            .with_time_in_force(TimeInForce::GoodTilCrossing),
        ),
        Err(PortfolioError::SubmitLimitOrder(
            SubmitLimitOrderError::GoodTillCrossingRejectedOrder { .. }
        ))
    ));
    assert!(
        pf.market(a)
            .unwrap()
            .active_limit_orders
            .get_by_id(amended.id(), Side::Buy)
            .is_some()
    );

    // 6. Partially fill 3 of the 5-unit amended order (remaining = 2), then amend total_qty
    //    from 5 -> 3 (qty_delta = -2, new_leaves_qty = 0) -> cancels order and returns
    //    AmendQtyAlreadyFilled.
    pf.update_market(
        a,
        &Trade {
            price: QuoteCurrency::new(93, 0),
            quantity: BaseCurrency::new(3, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 3.into(),
        },
    )
    .unwrap();
    assert!(matches!(
        pf.amend_limit_order(
            a,
            amended.id(),
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(94, 0),
                BaseCurrency::new(3, 0),
            )
            .unwrap(),
        ),
        Err(PortfolioError::AmendLimitOrder(
            AmendLimitOrderError::AmendQtyAlreadyFilled
        ))
    ));
    assert!(pf.market(a).unwrap().active_limit_orders.is_empty());
}

#[test]
fn portfolio_enforces_limit_submit_rate_limit_and_post_only_crossing_rejection() {
    let spec = ContractSpecification::new(
        leverage!(5),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        Fee::from(Decimal::zero()),
        Fee::from(Decimal::zero()),
    )
    .expect(EXPECT_CONTRACT_SPEC)
    .with_ticker("AAA");
    let rate_limits = OrderRateLimits::new(std::num::NonZeroU32::new(1).expect(EXPECT_NON_ZERO));
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
            spec,
            rate_limits,
        )],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();

    // Post-only crossing buy via submit_limit_order hits GoodTillCrossingRejectedOrder, not
    // WouldExecuteImmediately, and consumes the single rate-limit slot in this second.
    assert!(matches!(
        pf.submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap()
            .with_time_in_force(TimeInForce::GoodTilCrossing),
        ),
        Err(PortfolioError::SubmitLimitOrder(
            SubmitLimitOrderError::GoodTillCrossingRejectedOrder { .. }
        ))
    ));

    // Second submission in the same second is rejected by the rate limiter.
    assert!(matches!(
        pf.submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(90, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap(),
        ),
        Err(PortfolioError::SubmitLimitOrder(
            SubmitLimitOrderError::RateLimitReached(_)
        ))
    ));
}

#[test]
fn portfolio_unquoted_market_zeroes_upnl_and_add_market_grows_stop_event_capacity() {
    // 1. Fresh market with no quotes yet has zero unrealized PnL even after quotes arrive on
    //    another market, and does not falsely trigger stops on submit_stop_order.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(500, 0),
        vec![make_linear_market_cfg("AAA", 5)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();

    // Immediately triggered stop on quoted market A is rejected (covers stop_reference_price_for_slot).
    assert!(matches!(
        pf.submit_stop_order(
            a,
            StopOrder::stop_market(
                Side::Sell,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(1, 0),
                NoUserOrderId,
            )
            .unwrap()
            .with_trigger_price_type(TriggerPriceType::BestBidAsk),
        ),
        Err(PortfolioError::StopOrder(StopOrderError::StopWouldTrigger))
    ));

    // Dynamically add market B (max 20 orders) -> total stop capacity grows from 20 to 40.
    // If `add_market` used `*=` instead of `+=` or skipped growing stop capacity, emitting
    // > 20 stop events in a single portfolio liquidation would overflow or under-allocate.
    let small_cfg = |ticker: &str| {
        let spec = ContractSpecification::new(
            leverage!(5),
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
            PriceFilter::default(),
            QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::zero()),
        )
        .expect(EXPECT_CONTRACT_SPEC)
        .with_ticker(ticker);
        MarketConfig::new(
            NonZeroU16::new(2).expect(EXPECT_NON_ZERO),
            spec,
            OrderRateLimits::default(),
        )
    };
    let mut pf_cap = LinearPortfolio::new(
        PortfolioConfig::new(QuoteCurrency::new(100, 0), vec![small_cfg("AAA")]).unwrap(),
    );
    let ma = MarketId::new(0);
    let mb = pf_cap.add_market(small_cfg("BBB")).unwrap();
    // Unquoted market B has zero unrealized PnL.
    assert_eq!(pf_cap.total_unrealized_pnl(), QuoteCurrency::zero());
    pf_cap.update_market(ma, &bba(99, 100, 1)).unwrap();
    pf_cap.update_market(mb, &bba(99, 100, 1)).unwrap();
    // Place 2 stops on ma and 2 stops on mb (4 total > initial capacity 2).
    for m in [ma, mb] {
        for p in [50, 51] {
            pf_cap
                .submit_stop_order(
                    m,
                    StopOrder::stop_market(
                        Side::Sell,
                        QuoteCurrency::new(p, 0),
                        BaseCurrency::new(1, 0),
                        NoUserOrderId,
                    )
                    .unwrap()
                    .with_trigger_price_type(TriggerPriceType::Last),
                )
                .unwrap();
        }
    }
    // Delisting mb drains its 2 pending stops and emits ForcedCancel events.
    pf_cap
        .delist_market(mb, QuoteCurrency::new(100, 0))
        .unwrap();
    assert_eq!(pf_cap.stop_events().len(), 2);
    assert!(matches!(
        pf_cap.stop_events()[0].event,
        StopOrderEvent::Cancelled {
            reason: StopCancelReason::ForcedCancel,
            ..
        }
    ));
    // Updating a delisted market does not fire stops or fill resting orders.
    assert!(
        pf_cap
            .update_market(mb, &bba(10, 11, 2))
            .unwrap()
            .is_empty()
    );

    // Add a third market of capacity 1: with `+=`, total cap is 2 + 2 + 1 = 5; with `*=`,
    // total cap would stay 2 * 2 * 1 = 4, which would panic when liquidating 5 stops.
    let mc = pf_cap
        .add_market({
            let spec = ContractSpecification::new(
                leverage!(5),
                Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
                PriceFilter::default(),
                QuantityFilter::new(None, None, BaseCurrency::new(1, 2))
                    .expect(EXPECT_QUANTITY_FILTER),
                Fee::from(Decimal::zero()),
                Fee::from(Decimal::zero()),
            )
            .expect(EXPECT_CONTRACT_SPEC)
            .with_ticker("CCC");
            MarketConfig::new(
                NonZeroU16::new(1).expect(EXPECT_NON_ZERO),
                spec,
                OrderRateLimits::default(),
            )
        })
        .unwrap();
    pf_cap.update_market(mc, &bba(99, 100, 1)).unwrap();
    pf_cap
        .submit_stop_order(
            mc,
            StopOrder::stop_market(
                Side::Sell,
                QuoteCurrency::new(50, 0),
                BaseCurrency::new(1, 0),
                NoUserOrderId,
            )
            .unwrap()
            .with_trigger_price_type(TriggerPriceType::Last),
        )
        .unwrap();
    // Open a leveraged position on ma and liquidate via funding -> cancels 2 stops on ma + 1 on mc = 3 stops (> initial 2).
    pf_cap
        .submit_market_order(
            ma,
            MarketOrder::new(Side::Buy, BaseCurrency::new(4, 0)).unwrap(),
        )
        .unwrap();
    let liq = pf_cap
        .apply_funding(
            ma,
            FundingRate::from(Decimal::try_from_scaled(25, 2).expect(EXPECT_DECIMAL)),
            QuoteCurrency::new(100, 0),
        )
        .unwrap();
    assert!(matches!(
        liq.solvency,
        Solvency::Liquidated | Solvency::Bankrupt
    ));
    assert_eq!(pf_cap.stop_events().len(), 3);

    // Open a short on A and verify stop-protection collar cancels a gapped buy stop.
    let cfg_collar = PortfolioConfig::new(
        QuoteCurrency::new(500, 0),
        vec![{
            let spec = ContractSpecification::new(
                leverage!(5),
                Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
                PriceFilter::default(),
                QuantityFilter::new(None, None, BaseCurrency::new(1, 2))
                    .expect(EXPECT_QUANTITY_FILTER),
                Fee::from(Decimal::zero()),
                Fee::from(Decimal::zero()),
            )
            .expect(EXPECT_CONTRACT_SPEC)
            .with_ticker("AAA")
            .with_stop_protection(StopProtection::new(QuoteCurrency::new(2, 0)).unwrap());
            MarketConfig::new(
                NonZeroU16::new(20).expect(EXPECT_NON_ZERO),
                spec,
                OrderRateLimits::default(),
            )
        }],
    )
    .unwrap();
    let mut pf_c = LinearPortfolio::new(cfg_collar);
    pf_c.update_market(a, &bba(99, 100, 1)).unwrap();
    pf_c.submit_market_order(
        a,
        MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    pf_c.submit_stop_order(
        a,
        StopOrder::stop_market(
            Side::Buy,
            QuoteCurrency::new(105, 0),
            BaseCurrency::new(1, 0),
            NoUserOrderId,
        )
        .unwrap()
        .with_trigger_price_type(TriggerPriceType::BestBidAsk),
    )
    .unwrap();
    // Gap ask to 120 (> stop 105 + collar 2 = 107) -> cancelled with StopProtectionBand.
    pf_c.update_market(a, &bba(119, 120, 2)).unwrap();
    assert_eq!(pf_c.stop_events().len(), 1);
    assert!(matches!(
        pf_c.stop_events()[0].event,
        StopOrderEvent::Cancelled {
            reason: StopCancelReason::StopProtectionBand,
            ..
        }
    ));
    // Position remains short 1 because the stop was protected.
    assert_eq!(
        pf_c.market(a).unwrap().position.quantity(),
        BaseCurrency::new(-1, 0)
    );

    // Confirm short reduce-only market order strictly reducing succeeds under ReduceOnly status.
    pf_c.set_market_status(a, MarketStatus::ReduceOnly).unwrap();
    let settle = pf_c
        .submit_market_order(
            a,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();
    assert_eq!(settle.solvency, Solvency::Solvent);
    assert!(pf_c.market(a).unwrap().position.quantity().is_zero());
}

#[test]
fn portfolio_delisted_market_ignores_limit_fills_and_reduce_only_rejects_short_flip() {
    // 1. ReduceOnly market rejects a short-flipping buy (Buy 2 against Short 1), rejects a
    //    short-increasing sell (Sell 1 against Short 1), and accepts Buy 1.
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(500, 0),
        vec![make_linear_market_cfg("AAA", 5)],
    )
    .unwrap();
    let mut pf = LinearPortfolio::new(cfg);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    pf.set_market_status(a, MarketStatus::ReduceOnly).unwrap();

    // Selling 1 against Short 1 is NOT strictly reducing (must reject with MarketReduceOnly).
    assert!(matches!(
        pf.submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(1, 0)).unwrap(),
        ),
        Err(PortfolioError::MarketReduceOnly(m)) if m == a
    ));
    // Selling 2 against Long 1 flips to Short 1 (not strictly reducing -> MarketReduceOnly).
    let mut pf_long = LinearPortfolio::new(
        PortfolioConfig::new(
            QuoteCurrency::new(500, 0),
            vec![make_linear_market_cfg("AAA", 5)],
        )
        .unwrap(),
    );
    pf_long.update_market(a, &bba(99, 100, 1)).unwrap();
    pf_long
        .submit_market_order(
            a,
            MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
        )
        .unwrap();
    pf_long
        .set_market_status(a, MarketStatus::ReduceOnly)
        .unwrap();
    assert!(matches!(
        pf_long.submit_market_order(
            a,
            MarketOrder::new(Side::Sell, BaseCurrency::new(2, 0)).unwrap(),
        ),
        Err(PortfolioError::MarketReduceOnly(m)) if m == a
    ));
    // Buying 2 against Short 1 flips to Long 1: must be rejected with MarketReduceOnly.
    assert!(matches!(
        pf.submit_limit_order(
            a,
            LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(90, 0),
                BaseCurrency::new(2, 0),
            )
            .unwrap(),
        ),
        Err(PortfolioError::MarketReduceOnly(m)) if m == a
    ));
    // Limit Buy 1 @ 90 strictly reduces Short 1: accepted.
    pf.submit_limit_order(
        a,
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(90, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap(),
    )
    .unwrap();

    // 2. Apply funding with zero position and non-zero rate returns zero cashflow.
    let delist = pf.delist_market(a, QuoteCurrency::new(100, 0)).unwrap();
    assert_eq!(delist.solvency, Solvency::Solvent);
    let funding = pf
        .apply_funding(
            a,
            FundingRate::from(Decimal::try_from_scaled(5, 2).expect(EXPECT_DECIMAL)),
            QuoteCurrency::new(100, 0),
        )
        .unwrap();
    assert_eq!(funding.cashflow, QuoteCurrency::zero());
}

#[test]
fn portfolio_liquidation_with_clearance_fee_reconciles_ledger_and_add_market_sums_capacity() {
    // 1. Portfolio liquidation with positive clearance fee must record a negative ledger entry
    //    so `record_ledger_flows`'s exact-equity assertion passes (kills `delete -` on clearance_fee).
    let spec_with_liq_fee = |ticker: &str| {
        let spec = ContractSpecification::new(
            leverage!(5),
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
            PriceFilter::default(),
            QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::zero()),
        )
        .expect(EXPECT_CONTRACT_SPEC)
        .with_ticker(ticker)
        .with_liquidation_fee(Decimal::try_from_scaled(1, 2).expect(EXPECT_DECIMAL))
        .unwrap();
        MarketConfig::new(
            NonZeroU16::new(2).expect(EXPECT_NON_ZERO),
            spec,
            OrderRateLimits::default(),
        )
    };
    let mut pf = LinearPortfolio::new(
        PortfolioConfig::new(QuoteCurrency::new(25, 0), vec![spec_with_liq_fee("AAA")]).unwrap(),
    );
    pf.enable_ledger(64);
    let a = MarketId::new(0);
    pf.update_market(a, &bba(99, 100, 1)).unwrap();
    pf.submit_market_order(
        a,
        MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    // Bid drops to 84 -> equity = 25 - 16 = 9 < MM 8.4? Wait: MM = 8.4, so drop to 83 -> equity 8 < 8.3.
    let err = pf.update_market(a, &bba(83, 84, 2)).unwrap_err();
    assert!(matches!(err, PortfolioError::Risk(RiskError::Liquidate)));
    // Closed at 83 (realized -17 -> equity 8), clearance fee = 83 * 0.01 = 0.83 -> equity 7.17.
    assert_eq!(pf.balances().equity(), QuoteCurrency::new(717, 2));
    assert!(pf.ledger().unwrap().reconciles_to(pf.balances().equity()));

    // 2. Starting with capacity 1 and dynamically adding capacity 1 gives total 2 (`1 + 1 = 2`,
    //    whereas `1 * 1 = 1`), so liquidating 2 pending stops across both markets must succeed.
    let cap1 = |ticker: &str| {
        let spec = ContractSpecification::new(
            leverage!(5),
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
            PriceFilter::default(),
            QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::zero()),
        )
        .expect(EXPECT_CONTRACT_SPEC)
        .with_ticker(ticker);
        MarketConfig::new(
            NonZeroU16::new(1).expect(EXPECT_NON_ZERO),
            spec,
            OrderRateLimits::default(),
        )
    };
    let mut pf2 = LinearPortfolio::new(
        PortfolioConfig::new(QuoteCurrency::new(25, 0), vec![cap1("AAA")]).unwrap(),
    );
    let ma = MarketId::new(0);
    let mb = pf2.add_market(cap1("BBB")).unwrap();
    pf2.update_market(ma, &bba(99, 100, 1)).unwrap();
    pf2.update_market(mb, &bba(99, 100, 1)).unwrap();
    pf2.submit_market_order(
        ma,
        MarketOrder::new(Side::Buy, BaseCurrency::new(1, 0)).unwrap(),
    )
    .unwrap();
    for m in [ma, mb] {
        pf2.submit_stop_order(
            m,
            StopOrder::stop_market(
                Side::Sell,
                QuoteCurrency::new(50, 0),
                BaseCurrency::new(1, 0),
                NoUserOrderId,
            )
            .unwrap()
            .with_trigger_price_type(TriggerPriceType::Last),
        )
        .unwrap();
    }
    let err2 = pf2.update_market(ma, &bba(83, 84, 2)).unwrap_err();
    assert!(matches!(err2, PortfolioError::Risk(RiskError::Liquidate)));
    assert_eq!(pf2.stop_events().len(), 2);
}
