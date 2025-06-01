//! Benchmark regarding checking of active limit orders.

use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use lfest::prelude::*;
use rand::{Rng, SeedableRng, rngs::SmallRng};

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

    for pos in starting_positions {
        for side in [Side::Buy, Side::Sell] {
            group.bench_function(&format!("{pos}_{side}_1000"), |b| {
                b.iter_with_setup(
                    || (pos.clone(), Balances::new(QuoteCurrency::new(1000, 0))),
                    |(mut position, mut balances)| {
                        for (filled_qty, fill_price) in random_changes.iter() {
                            black_box(position.change(
                                *filled_qty,
                                *fill_price,
                                side,
                                &mut balances,
                                Decimal::ONE,
                            ));
                        }
                    },
                )
            });
        }
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
