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

use const_decimal::Decimal;
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

    let init_margin_req = Decimal::ONE;
    let max_active_orders = NonZeroU16::new(100).unwrap();

    for n in 1..20 {
        group.bench_with_input(BenchmarkId::new("insert", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
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
                            .try_insert_order(black_box(order.clone()), init_margin_req)
                            .expect("Can insert")
                    }
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("fill_order", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account
                            .try_insert_order(order.clone(), init_margin_req)
                            .expect("Can insert");
                    }
                    account
                },
                |mut account| {
                    for order in orders.iter() {
                        account.fill_order(black_box(order), init_margin_req)
                    }
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("remove", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account
                            .try_insert_order(black_box(order.clone()), init_margin_req)
                            .unwrap()
                    }
                    account
                },
                |mut account| {
                    for order in orders.iter() {
                        account
                            .remove_limit_order(
                                black_box(CancelBy::OrderId(order.id())),
                                init_margin_req,
                            )
                            .expect("Can insert");
                    }
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_neutral", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account
                            .try_insert_order(black_box(order.clone()), init_margin_req)
                            .unwrap()
                    }
                    account
                },
                |account| {
                    let _ = black_box(account.order_margin(init_margin_req));
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_long", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account
                            .try_insert_order(black_box(order.clone()), init_margin_req)
                            .unwrap()
                    }
                    account
                },
                |account| {
                    let _ = black_box(account.order_margin(init_margin_req));
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_short", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut account = Account::new(
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                        max_active_orders,
                    );
                    for order in orders.iter() {
                        account
                            .try_insert_order(black_box(order.clone()), init_margin_req)
                            .unwrap()
                    }
                    account
                },
                |account| {
                    let _ = black_box(account.order_margin(init_margin_req));
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
