//! Benchmark regarding `OrderMargin`

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::{
    hint::black_box,
    num::NonZeroU16,
};

use criterion::{
    BenchmarkId,
    Criterion,
    criterion_group,
    criterion_main,
};
use lfest::prelude::*;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("OrderMargin");

    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::<i64, 5>::new(99, 0),
        BaseCurrency::new(1, 1),
    )
    .unwrap();

    let max_active_orders = NonZeroU16::new(100).unwrap();

    for n in 1..20 {
        group.bench_with_input(BenchmarkId::new("insert", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    )
                },
                |mut account| {
                    for order in orders.iter() {
                        account
                            .try_insert_order(black_box(order.clone()))
                            .expect("Can insert")
                    }
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("fill_order", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account.try_insert_order(order.clone()).expect("Can insert");
                    }
                    account
                },
                |mut account| {
                    for order in orders.iter() {
                        let _ = account.fill_order(
                            black_box(order.id()),
                            black_box(order.side()),
                            black_box(order.filled_quantity()),
                            black_box(order.limit_price()),
                            black_box(QuoteCurrency::new(1, 4)),
                            black_box(0.into()),
                        );
                    }
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("remove", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account.try_insert_order(black_box(order.clone())).unwrap()
                    }
                    account
                },
                |mut account| {
                    for order in orders.iter() {
                        account
                            .cancel_limit_order(black_box(CancelBy::OrderId(order.id())))
                            .expect("Can insert");
                    }
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_neutral", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account.try_insert_order(black_box(order.clone())).unwrap()
                    }
                    account
                },
                |account| {
                    let _ = black_box(account.order_margin());
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_long", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account.try_insert_order(black_box(order.clone())).unwrap()
                    }
                    account
                },
                |account| {
                    let _ = black_box(account.order_margin());
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_short", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account.try_insert_order(black_box(order.clone())).unwrap()
                    }
                    account
                },
                |account| {
                    let _ = black_box(account.order_margin());
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
