//! Benchmark the end to end order lifecycle: submit, amend, cancel.
//!
//! The per-operation benchmarks measure one call in isolation. This one does
//! the full round trip on a realistic book (100 resting orders), so the
//! reported throughput is directly comparable to the orders/second claim in
//! the README.

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

mod common;

use std::num::NonZeroU16;

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
/// Resting orders already in the book while the lifecycle runs.
const BACKLOG: usize = 100;
const BACKLOG_PRICE: i64 = 51;
/// The price of the order that goes through the lifecycle.
const CYCLE_PRICE: i64 = 95;
const AMENDED_PRICE: i64 = 96;

type ThisExchange = Exchange<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>;

type ThisOrder = LimitOrder<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId, NewOrder>;

fn order(price: i64) -> ThisOrder {
    LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(price, 0),
        BaseCurrency::new(ORDER_QTY, QTY_SCALE),
    )
    .expect("order is valid")
}

/// A fresh exchange with a passive backlog.
///
/// The backlog makes `get_by_id`, `remove_by_id` and the insert pay realistic
/// book costs instead of scanning a single order.
fn exchange_with_backlog() -> ThisExchange {
    let mut exchange =
        common::linear_exchange(NonZeroU16::new(MAX_ACTIVE_ORDERS).expect("200 is not zero"));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .expect("is valid market update");
    for i in 0..BACKLOG {
        // Spread the backlog over the price range below the bid so the cycle
        // order does not always land at the same index.
        let price = BACKLOG_PRICE + (i % 40) as i64;
        exchange
            .submit_limit_order(order(price))
            .expect("Can submit limit order");
    }
    exchange
}

fn bench_lifecycle(c: &mut Criterion) {
    let mut group = common::group(c, "Exchange");

    for n in common::sweep(100) {
        // One submit, one amend and one cancel per cycle: report operations,
        // not cycles, so the throughput is comparable across benchmarks.
        group.throughput(Throughput::Elements(3 * n as u64));
        group.bench_function(BenchmarkId::new("submit_amend_cancel", n), |b| {
            b.iter_with_setup(exchange_with_backlog, |mut exchange| {
                for _ in 0..n {
                    let submitted = exchange
                        .submit_limit_order(order(CYCLE_PRICE))
                        .expect("Can submit limit order");
                    let amended = exchange
                        .amend_limit_order(submitted.id(), order(AMENDED_PRICE))
                        .expect("Can amend limit order");
                    exchange
                        .cancel_limit_order(CancelBy::OrderId(amended.id()))
                        .expect("Can cancel limit order");
                }
                assert_eq!(
                    exchange.account().active_limit_orders().len(),
                    BACKLOG,
                    "every cycle leaves exactly the backlog behind"
                );
            })
        });
    }
}

criterion_group!(benches, bench_lifecycle);
criterion_main!(benches);
