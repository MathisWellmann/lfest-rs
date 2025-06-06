//! Benchmark the submission of limit orders.

use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lfest::prelude::*;
use rand::{Rng, SeedableRng, rngs::SmallRng};

const DECIMALS: u8 = 5;

type ThisExchange = Exchange<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId>;

fn setup_exchange() -> ThisExchange {
    let starting_balance = BaseCurrency::new(100000, 0);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(5, 1),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .expect("is valid filter"),
        QuantityFilter::new(None, None, QuoteCurrency::new(1, 1)).expect("is valid filter"),
        Fee::from(Decimal::try_from_scaled(2, 4).unwrap()),
        Fee::from(Decimal::try_from_scaled(6, 4).unwrap()),
    )
    .expect("works");
    let config = Config::new(
        starting_balance,
        200,
        contract_spec,
        OrderRateLimits::new(u16::MAX).unwrap(),
    )
    .unwrap();
    let mut exchange = Exchange::<i64, 5, QuoteCurrency<i64, 5>, NoUserOrderId>::new(config);
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .expect("is valid market update");
    exchange
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Exchange");

    let mut rng = SmallRng::seed_from_u64(0);

    for n in (1..100).step_by(5) {
        let orders = Vec::from_iter((0..n).map(|_| {
            if rng.random() {
                LimitOrder::new(
                    Side::Buy,
                    QuoteCurrency::new(scale(0.0, 1.0, 60.0, 90.0, rng.random()) as i64, 0),
                    QuoteCurrency::new(scale(0.0, 1.0, 1.0, 1000.0, rng.random()) as i64, 0),
                )
                .unwrap()
            } else {
                LimitOrder::new(
                    Side::Sell,
                    QuoteCurrency::new(scale(0.0, 1.0, 110.0, 150.0, rng.random()) as i64, 0),
                    QuoteCurrency::new(scale(0.0, 1.0, 1.0, 1000.0, rng.random()) as i64, 0),
                )
                .unwrap()
            }
        }));
        group.throughput(criterion::Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("submit_limit_order", n), &n, |b, _n| {
            b.iter_with_setup(
                || setup_exchange(),
                |mut exchange| {
                    for order in orders.iter() {
                        exchange
                            .submit_limit_order(black_box(order.clone()))
                            .expect("Can submit market order");
                    }
                },
            )
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
