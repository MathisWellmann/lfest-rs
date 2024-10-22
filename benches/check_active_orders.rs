use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{criterion_group, criterion_main, Criterion};
use lfest::prelude::*;

const DECIMALS: u8 = 5;

fn criterion_benchmark(c: &mut Criterion) {
    // Technically the setup code should not be benchmarked.
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
        QuantityFilter::new(None, None, QuoteCurrency::new(1, 1)).expect("is valid filter"),
        Fee::from(Decimal::try_from_scaled(2, 4).unwrap()),
        Fee::from(Decimal::try_from_scaled(6, 4).unwrap()),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();

    let mut group = c.benchmark_group("check_active_orders");

    for n in [1, 2, 3, 5, 10, 100] {
        group.bench_function(&format!("{n}"), |b| {
            let mut exchange = Exchange::<
                i64,
                DECIMALS,
                QuoteCurrency<i64, DECIMALS>,
                (),
                InMemoryTransactionAccounting<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>,
                NoAccountTracker,
            >::new(acc_tracker.clone(), config.clone());
            let ts_ns: TimestampNs = 0.into();
            let market_update = bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
            exchange
                .update_state(ts_ns, &market_update)
                .expect("is valid market update");
            let order = LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                QuoteCurrency::new(1, 1),
            )
            .unwrap();
            for _ in 0..n {
                exchange.submit_limit_order(order.clone()).unwrap();
            }
            b.iter(|| {
                exchange.check_active_orders(black_box(&market_update), black_box(ts_ns));
            })
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
