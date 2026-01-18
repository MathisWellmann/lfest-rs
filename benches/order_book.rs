//! Benchmark regarding order book implementation.

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::{
    hint::black_box,
    num::NonZeroU16,
};

use const_decimal::Decimal;
use criterion::{
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use lfest::prelude::*;
use rand::{
    Rng,
    SeedableRng,
};

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("OrderBook");

    let mut rng = rand::rngs::SmallRng::seed_from_u64(0);
    let init_margin_req = Decimal::one();

    for n in 1..100 {
        group.throughput(Throughput::Elements(n));
        group.bench_with_input(BenchmarkId::new("try_insert_order", n), &n, |b, _| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                let order = LimitOrder::new(
                    Side::Buy,
                    QuoteCurrency::<i64, 5>::new(rng.random_range(50..200), 0),
                    BaseCurrency::new(rng.random_range(1..100), 2),
                )
                .unwrap();
                order.into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    Account::new(
                        Balances::new(QuoteCurrency::new(1000, 0)),
                        NonZeroU16::new(n as u16).unwrap(),
                    )
                },
                |mut ob| {
                    for order in orders.iter() {
                        ob.try_insert_order(black_box(order.clone()), init_margin_req)
                            .expect("Can insert");
                    }
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
