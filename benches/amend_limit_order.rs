//! Benchmark amending limit orders.
//!
//! `Exchange::amend_limit_order` cancels the existing order and submits a
//! replacement, so it covers the linear `get_by_id` scan, the removal from the
//! sorted book, the risk check and the re-insert. Nothing else in the suite
//! touches this path.

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
/// 0.02 base currency, an increase over `ORDER_QTY`.
const AMENDED_QTY: i64 = 2;
const MAX_ACTIVE_ORDERS: u16 = 200;
const LIMIT_PRICE: i64 = 99;
const AMENDED_PRICE: i64 = 98;

type ThisExchange = Exchange<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>;

type ThisOrder = LimitOrder<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId, NewOrder>;

fn order(price: i64, quantity: i64) -> ThisOrder {
    LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(price, 0),
        BaseCurrency::new(quantity, QTY_SCALE),
    )
    .expect("order is valid")
}

/// A fresh exchange with `n` resting bids, plus their ids in insertion order.
///
/// Amending order `i` scans the book up to the position where `i` was
/// inserted, so the sweep over `n` measures the worst case scan cost.
fn exchange_with_resting(n: usize) -> (ThisExchange, Vec<OrderId>) {
    let mut exchange =
        common::linear_exchange(NonZeroU16::new(MAX_ACTIVE_ORDERS).expect("200 is not zero"));
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .expect("is valid market update");
    let ids = Vec::from_iter((0..n).map(|_| {
        exchange
            .submit_limit_order(order(LIMIT_PRICE, ORDER_QTY))
            .expect("Can submit limit order")
            .id()
    }));
    assert_eq!(ids.len(), n);
    (exchange, ids)
}

/// Amend every resting order once, moving it to a different price.
fn bench_amend_price(c: &mut Criterion) {
    let mut group = common::group(c, "Exchange");

    for n in common::sweep(100) {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(BenchmarkId::new("amend_price", n), |b| {
            b.iter_with_setup(
                || exchange_with_resting(n),
                |(mut exchange, ids)| {
                    for id in ids {
                        let amended = exchange
                            .amend_limit_order(id, order(AMENDED_PRICE, ORDER_QTY))
                            .expect("Can amend limit order");
                        black_box(amended.id());
                    }
                },
            )
        });
    }
}

/// Amend every resting order once, increasing its quantity.
///
/// Takes the quantity-delta branch of `amend_limit_order` instead of the
/// remove-and-reinsert-price path.
fn bench_amend_quantity(c: &mut Criterion) {
    let mut group = common::group(c, "Exchange");

    for n in common::sweep(100) {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(BenchmarkId::new("amend_quantity", n), |b| {
            b.iter_with_setup(
                || exchange_with_resting(n),
                |(mut exchange, ids)| {
                    for id in ids {
                        let amended = exchange
                            .amend_limit_order(id, order(LIMIT_PRICE, AMENDED_QTY))
                            .expect("Can amend limit order");
                        black_box(amended.id());
                    }
                },
            )
        });
    }
}

criterion_group!(benches, bench_amend_price, bench_amend_quantity);
criterion_main!(benches);
