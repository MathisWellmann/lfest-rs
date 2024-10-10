use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use lfest::prelude::*;

fn submit_market_orders<Q, U>(
    exchange: &mut Exchange<
        NoAccountTracker,
        Q,
        (),
        InMemoryTransactionAccounting<Q::PairedCurrency>,
    >,
    order: &MarketOrder<Q, (), NewOrder>,
    n: usize,
) where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    for _ in 0..n {
        exchange
            .submit_market_order(order.clone())
            .expect("Can submit market order");
    }
}

fn criterion_benchmark(c: &mut Criterion) {
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
    let mut exchange = Exchange::new(acc_tracker, config);
    exchange
        .update_state(0.into(), &bba!(quote!(100), quote!(101)))
        .expect("is valid market update");

    let order = MarketOrder::new(Side::Buy, quote!(0.1)).unwrap();
    let mut group = c.benchmark_group("submit_market_order");
    const N: usize = 1_000;
    group.throughput(criterion::Throughput::Elements(N as u64));
    group.bench_function(&format!("submit_market_order_{N}"), |b| {
        b.iter(|| {
            submit_market_orders::<QuoteCurrency, Trade<QuoteCurrency>>(
                black_box(&mut exchange),
                black_box(&order),
                N,
            )
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
