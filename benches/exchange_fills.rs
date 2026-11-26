//! Benchmark the path where a market update fills resting limit orders.
//!
//! Every other exchange benchmark runs against a book that never trades: the
//! trades are priced away from the resting orders, so `fill_limit_order`, the
//! taker fee accounting, the PnL settlement, the margin reconciliation and the
//! forced cancels after a margin call were never measured. These benchmarks
//! price the trade strictly through the resting bids so every update fills.

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

mod common;

use std::{
    hint::black_box,
    num::NonZeroU16,
};

use criterion::{
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use lfest::prelude::*;

const QTY_SCALE: u8 = 2;
/// 0.01 base currency, the minimum the quantity filter allows.
const ORDER_QTY: i64 = 1;
const MAX_ACTIVE_ORDERS: u16 = 200;
/// The resting bid. A sell trade priced strictly below it fills.
const LIMIT_PRICE: i64 = 99;
const FILL_PRICE: i64 = 98;

type ThisExchange = Exchange<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>;

fn buy_order() -> LimitOrder<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId, NewOrder> {
    LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(LIMIT_PRICE, 0),
        BaseCurrency::new(ORDER_QTY, QTY_SCALE),
    )
    .expect("order is valid")
}

fn trade(quantity: i64) -> Trade<i64, 5, BaseCurrency<i64, 5>> {
    Trade {
        timestamp_exchange_ns: 1.into(),
        price: QuoteCurrency::new(FILL_PRICE, 0),
        quantity: BaseCurrency::new(quantity, QTY_SCALE),
        side: Side::Sell,
    }
}

/// A fresh exchange with `n` resting bids and a known bid/ask.
fn exchange_with_resting(n: usize) -> ThisExchange {
    let mut exchange =
        common::linear_exchange(NonZeroU16::new(MAX_ACTIVE_ORDERS).expect("200 is not zero"));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .expect("is valid market update");
    for _ in 0..n {
        exchange
            .submit_limit_order(buy_order())
            .expect("Can submit limit order");
    }
    exchange
}

/// One trade per fill, through `update_state`.
///
/// This is the backtest hot loop: market state update, maintenance margin
/// check, fill, fee, PnL, margin reconciliation.
fn bench_update_state_fills(c: &mut Criterion) {
    let mut group = common::group(c, "Exchange");

    for n in common::sweep(100) {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(BenchmarkId::new("update_state_fills", n), |b| {
            let trade = trade(ORDER_QTY);
            b.iter_with_setup(
                || exchange_with_resting(n),
                |mut exchange| {
                    for _ in 0..n {
                        exchange
                            .update_state(&trade)
                            .expect("is a valid market update");
                    }
                },
            )
        });
    }
}

/// A single trade whose quantity sweeps `n` resting bids in one call.
///
/// Isolates the fill loop from the market state update and the maintenance
/// margin check that `update_state` performs around it.
fn bench_check_active_orders_fills(c: &mut Criterion) {
    let mut group = common::group(c, "Exchange");

    for n in common::sweep(100) {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(BenchmarkId::new("check_active_orders_fills", n), |b| {
            let trade = trade(ORDER_QTY * n as i64);
            b.iter_with_setup(
                || exchange_with_resting(n),
                |mut exchange| {
                    exchange.check_active_orders(trade);
                    black_box(exchange.limit_order_events().len());
                },
            )
        });
    }
}

criterion_group!(
    benches,
    bench_update_state_fills,
    bench_check_active_orders_fills
);
criterion_main!(benches);
