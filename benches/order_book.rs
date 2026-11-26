//! Benchmark regarding order book implementation.

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
use lfest::{
    prelude::*,
    test_fee_maker,
};
use rand::{
    RngExt,
    SeedableRng,
    rngs::SmallRng,
};

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = common::group(c, "OrderBook");

    let mut rng = SmallRng::seed_from_u64(0);

    for n in common::sweep(100) {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(BenchmarkId::new("try_insert_order", n), |b| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                LimitOrder::new(
                    Side::Buy,
                    QuoteCurrency::<i64, 5>::new(rng.random_range(50..200), 0),
                    BaseCurrency::new(rng.random_range(1..100), 2),
                )
                .expect("is valid order")
                .into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    Account::new(
                        // Must cover the notional of every order, otherwise the
                        // larger sweep points would fail on missing margin.
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        NonZeroU16::new(u16::try_from(n).expect("sweep stays below u16::MAX"))
                            .expect("sweep never yields zero"),
                        const_decimal::Decimal::ONE,
                        *test_fee_maker().as_ref(),
                    )
                },
                |mut ob| {
                    for order in orders.iter() {
                        ob.try_insert_order(black_box(order.clone()))
                            .expect("Can insert");
                    }
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
