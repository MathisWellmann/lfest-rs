//! Benchmark regarding checking of active limit orders.

use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{Criterion, criterion_group, criterion_main};
use lfest::prelude::*;

const DECIMALS: u8 = 5;

fn criterion_benchmark(c: &mut Criterion) {
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
        OrderRateLimits::new(1_000).unwrap(),
    )
    .unwrap();

    let mut group = c.benchmark_group("check_active_orders");

    let mut ts_s = 0;
    for n in [1, 2, 3, 5, 10, 100] {
        group.bench_function(&format!("{n}"), |b| {
            let mut exchange =
                Exchange::<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId>::new(
                    config.clone(),
                );
            let bba = Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: ts_s.into(),
            };
            ts_s += 1;
            exchange.update_state(&bba).expect("is valid market update");
            let order = LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(99, 0),
                QuoteCurrency::new(1, 1),
            )
            .unwrap();
            for _ in 0..n {
                exchange.submit_limit_order(order.clone()).unwrap();
            }
            let trade = Trade {
                timestamp_exchange_ns: 0.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: QuoteCurrency::new(10, 0),
                side: Side::Sell,
            };
            b.iter(|| {
                let _ = black_box(exchange.check_active_orders(black_box(trade)));
            })
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
