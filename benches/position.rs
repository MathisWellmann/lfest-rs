//! Benchmark regarding checking of active limit orders.

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::hint::black_box;

use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use lfest::prelude::*;
use rand::{
    Rng,
    SeedableRng,
    rngs::SmallRng,
};

fn criterion_benchmark(c: &mut Criterion) {
    const N: usize = 1000;

    let mut group = c.benchmark_group("Position");
    group.throughput(Throughput::Elements(N as u64));

    let mut rng = SmallRng::seed_from_u64(0);
    let starting_positions = [
        Position::Neutral,
        Position::Long(PositionInner::new(
            BaseCurrency::<i64, 5>::new(1, 0),
            QuoteCurrency::new(100, 0),
        )),
        Position::Short(PositionInner::new(
            BaseCurrency::new(1, 0),
            QuoteCurrency::new(100, 0),
        )),
    ];
    let random_changes = Vec::from_iter((0..N).map(|_| {
        (
            BaseCurrency::<i64, 5>::new(scale(0.0, 1.0, 1.0, 100.0, rng.random()) as i64, 2),
            QuoteCurrency::new(scale(0.0, 1.0, 50.0, 100.0, rng.random()) as i64, 0),
        )
    }));
    let bid = QuoteCurrency::new(100, 0);
    let ask = QuoteCurrency::new(101, 0);

    for pos in starting_positions {
        group.bench_function(format!("{pos}_unrealized_pnl"), |b| {
            b.iter(|| {
                let _ = black_box(pos.unrealized_pnl(bid, ask));
            })
        });
        group.bench_function(format!("{pos}_quantity"), |b| {
            b.iter(|| {
                let _ = black_box(pos.quantity());
            })
        });
        group.bench_function(format!("{pos}_entry_price"), |b| {
            b.iter(|| {
                let _ = black_box(pos.entry_price());
            })
        });
        group.bench_function(format!("{pos}_notional"), |b| {
            b.iter(|| {
                let _ = black_box(pos.notional());
            })
        });

        for side in [Side::Buy, Side::Sell] {
            group.bench_function(format!("{pos}_{side}_1000_change_position"), |b| {
                b.iter_with_setup(
                    || (pos.clone(), Balances::new(QuoteCurrency::new(1000, 0))),
                    |(mut position, mut balances)| {
                        for (filled_qty, fill_price) in random_changes.iter() {
                            let _: () =
                                position.change(*filled_qty, *fill_price, side, &mut balances);
                            black_box(());
                        }
                    },
                )
            });
        }
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
