use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use lfest::prelude::*;

fn submit_limit_orders<U>(order: &LimitOrder<Decimal, Quote, (), NewOrder>, n: usize) {
    // Technically the setup code should not be benchmarked.
    let starting_balance = base!(100000);
    let acc_tracker = NoAccountTracker::default();
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::new(None, None, quote!(0.5), Dec!(2), Dec!(0.5)).expect("is valid filter"),
        QuantityFilter::new(None, None, quote!(0.1)).expect("is valid filter"),
        Fee::from_basis_points(2),
        Fee::from_basis_points(6),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    let mut exchange = Exchange::<
        NoAccountTracker,
        Decimal,
        Quote,
        (),
        InMemoryTransactionAccounting<Decimal, Base>,
    >::new(acc_tracker, config);
    exchange
        .update_state(0.into(), &bba!(quote!(100), quote!(101)))
        .expect("is valid market update");
    for _ in 0..n {
        exchange
            .submit_limit_order(order.clone())
            .expect("Can submit market order");
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let order = LimitOrder::new(Side::Buy, quote!(100), quote!(0.1)).unwrap();
    let mut group = c.benchmark_group("submit_limit_order");

    // TODO: clearly the performance of `exchange::submit_limit_order` leaves a lot to be desired.

    let n: usize = 1;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| submit_limit_orders::<Trade<Decimal, Quote>>(black_box(&order), n))
    });

    let n: usize = 10;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| submit_limit_orders::<Trade<Decimal, Quote>>(black_box(&order), n))
    });

    let n: usize = 100;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| submit_limit_orders::<Trade<Decimal, Quote>>(black_box(&order), n))
    });

    let n: usize = 1000;
    group.throughput(criterion::Throughput::Elements(n as u64));
    group.bench_function(&format!("submit_limit_order_{n}"), |b| {
        b.iter(|| submit_limit_orders::<Trade<Decimal, Quote>>(black_box(&order), n))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
