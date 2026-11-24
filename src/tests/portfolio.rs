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
