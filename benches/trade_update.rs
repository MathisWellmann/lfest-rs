#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::hint::black_box;

use criterion::*;
use lfest::prelude::*;
use rand::{
    Rng,
    SeedableRng,
    rngs::SmallRng,
};

fn criterion_benchmark(c: &mut Criterion) {
    const MAX_CAP: usize = 10;

    let mut group = c.benchmark_group("Trade");

    let mut rng = SmallRng::seed_from_u64(0);

    let test_bids = Vec::from_iter((0..MAX_CAP).map(|i| {
        let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 6>::new(rng.random_range(1..10_000), 0),
            BaseCurrency::new(rng.random_range(1..1_000), 2),
        )
        .unwrap()
        .into_pending(meta)
    }));

    let trade = Trade {
        timestamp_exchange_ns: 0.into(),
        price: QuoteCurrency::new(100, 0),
        quantity: BaseCurrency::new(1, 0),
        side: Side::Sell,
    };

    for cap in 1..MAX_CAP {
        group.throughput(Throughput::Elements(cap as u64));
        group.bench_function(format!("fills_order_{cap}"), |b| {
            b.iter(|| {
                for order in test_bids.iter() {
                    let _ = black_box(trade.fills_order(black_box(order)));
                }
            })
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
