use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{criterion_group, criterion_main, Criterion};
use lfest::prelude::*;

const DECIMALS: u8 = 5;

fn submit_market_orders<I, const D: u8, BaseOrQuote, U>(
    exchange: &mut Exchange<
        I,
        D,
        BaseOrQuote,
        (),
        InMemoryTransactionAccounting<I, D, BaseOrQuote::PairedCurrency>,
        NoAccountTracker,
    >,
    order: &MarketOrder<I, D, BaseOrQuote, (), NewOrder>,
    n: usize,
) where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    for _ in 0..n {
        exchange
            .submit_market_order(order.clone())
            .expect("Can submit market order");
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let starting_balance = BaseCurrency::new(100000, 0);
    let acc_tracker = NoAccountTracker::default();
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
        QuantityFilter::new(None, None, QuoteCurrency::new(1, 2)).expect("is valid filter"),
        Fee::from(Decimal::try_from_scaled(2, 1).unwrap()),
        Fee::from(Decimal::try_from_scaled(6, 1).unwrap()),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    let mut exchange = Exchange::new(acc_tracker, config);
    exchange
        .update_state(
            0.into(),
            &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0)),
        )
        .expect("is valid market update");

    let order = MarketOrder::new(Side::Buy, QuoteCurrency::new(1, 2)).unwrap();
    let mut group = c.benchmark_group("submit_market_order");
    const N: usize = 1_000;
    group.throughput(criterion::Throughput::Elements(N as u64));
    group.bench_function(&format!("submit_market_order_{N}"), |b| {
        b.iter(|| {
            submit_market_orders::<
                i64,
                DECIMALS,
                QuoteCurrency<i64, DECIMALS>,
                Trade<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>,
            >(black_box(&mut exchange), black_box(&order), N)
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
