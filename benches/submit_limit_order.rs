//! Benchmark the submission of limit orders.

use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{Criterion, criterion_group, criterion_main};
use lfest::prelude::*;

const DECIMALS: u8 = 5;

fn submit_limit_orders<U>(
    order: &LimitOrder<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder>,
    n: usize,
) {
    // Technically the setup code should not be benchmarked.
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
        OrderRateLimits::new(n as _).unwrap(),
    )
    .unwrap();
    let mut exchange = Exchange::<
        i64,
        5,
        QuoteCurrency<i64, 5>,
        NoUserOrderId,
        InMemoryTransactionAccounting<i64, 5, BaseCurrency<i64, 5>>,
    >::new(config);
    exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .expect("is valid market update");
    for _ in 0..n {
        exchange
            .submit_limit_order(order.clone())
            .expect("Can submit market order");
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        QuoteCurrency::new(1, 1),
    )
    .unwrap();
    let mut group = c.benchmark_group("submit_limit_order");

    let n: usize = 1;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| {
            submit_limit_orders::<Trade<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>>(
                black_box(&order),
                n,
            )
        })
    });

    let n: usize = 10;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| {
            submit_limit_orders::<Trade<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>>(
                black_box(&order),
                n,
            )
        })
    });

    let n: usize = 100;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| {
            submit_limit_orders::<Trade<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>>(
                black_box(&order),
                n,
            )
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
