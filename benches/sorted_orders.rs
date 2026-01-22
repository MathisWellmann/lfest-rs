#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::{
    hint::black_box,
    num::NonZeroU16,
};

use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use lfest::{
    EXPECT_NON_ZERO,
    prelude::*,
    sorted_orders::{
        Bids,
        SortedOrders,
    },
};
use rand::{
    Rng,
    SeedableRng,
    rngs::SmallRng,
};

fn criterion_benchmark(c: &mut Criterion) {
    const MAX_CAP: usize = 10;

    let mut group = c.benchmark_group("SortedOrders");

    let mut rng = SmallRng::seed_from_u64(0);

    let test_bids = Vec::from_iter((0..MAX_CAP).map(|i| {
        let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(rng.random_range(1..10_000), 0),
            BaseCurrency::new(rng.random_range(1..1_000), 2),
        )
        .unwrap()
        .into_pending(meta)
    }));

    for cap in 1..MAX_CAP {
        group.throughput(Throughput::Elements(cap as u64));
        group.bench_function(format!("with_capacity_{cap}_try_insert"), |b| {
            b.iter_with_setup(
                || {
                    SortedOrders::<i64, 6, BaseCurrency<i64, 6>, NoUserOrderId, Bids>::with_capacity(
                        NonZeroU16::new(cap.try_into().unwrap()).expect(EXPECT_NON_ZERO),
                    )
                },
                |mut orders| {
                    for i in 0..cap {
                        let _ = black_box(orders.try_insert(black_box(test_bids[i].clone())));
                    }
                },
            )
        });
        group.bench_function(format!("with_capacity_{cap}_remove_by_id"), |b| {
            b.iter_with_setup(
                || {
                    let mut bids = SortedOrders::<i64, 6, BaseCurrency<i64, 6>, NoUserOrderId, Bids>::with_capacity(
                        NonZeroU16::new(cap.try_into().unwrap()).expect(EXPECT_NON_ZERO),
                    );
                    for i in 0..cap {
                        let _ = black_box(bids.try_insert(black_box(test_bids[i].clone())));
                    }
                    bids
                },
                |mut orders| {
                    for i in 0..cap {
                        let _ = black_box(orders.remove_by_id((i as u64).into()));
                    }
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
