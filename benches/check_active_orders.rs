//! Benchmark regarding checking of active limit orders.

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::{
    hint::black_box,
    num::{
        NonZeroU32,
        NonZeroUsize,
    },
};

use const_decimal::Decimal;
use criterion::{
    BenchmarkId,
    Criterion,
    criterion_group,
    criterion_main,
};
use lfest::{
    EXPECT_NON_ZERO,
    prelude::*,
};

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
        NonZeroUsize::new(200).unwrap(),
        contract_spec,
        OrderRateLimits::new(NonZeroU32::new(1_000).expect(EXPECT_NON_ZERO)),
    )
    .unwrap();

    let mut group = c.benchmark_group("Exchange");

    let trade = Trade {
        timestamp_exchange_ns: 0.into(),
        price: QuoteCurrency::<i64, 5>::new(100, 0),
        quantity: QuoteCurrency::new(10, 0),
        side: Side::Sell,
    };

    for n in 1..50 {
        group.bench_with_input(BenchmarkId::new("check_active_orders", n), &n, |b, _n| {
            b.iter_with_setup(
                || {
                    let mut exchange = Exchange::<
                        i64,
                        DECIMALS,
                        QuoteCurrency<i64, DECIMALS>,
                        NoUserOrderId,
                    >::new(config.clone());
                    let bba = Bba {
                        bid: QuoteCurrency::new(100, 0),
                        ask: QuoteCurrency::new(101, 0),
                        timestamp_exchange_ns: 1.into(),
                    };
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
                    exchange
                },
                |mut exchange| {
                    let _: () = exchange.check_active_orders(black_box(trade));
                    black_box(());
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
