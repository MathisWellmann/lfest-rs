//! Benchmark regarding checking of active limit orders.

use std::{hint::black_box, num::NonZeroUsize};

use const_decimal::Decimal;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lfest::prelude::*;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("OrderMargin");

    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::<i64, 5>::new(99, 0),
        BaseCurrency::new(1, 1),
    )
    .unwrap();

    let position = Position::Neutral;
    let init_margin_req = Decimal::ONE;

    for n in 1..20 {
        group.bench_with_input(BenchmarkId::new("insert", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    (
                        OrderMargin::new(NonZeroUsize::new(n).unwrap()),
                        Balances::new(QuoteCurrency::new(1_000_000, 0)),
                    )
                },
                |(mut order_margin, mut balances)| {
                    for order in orders.iter() {
                        order_margin
                            .try_insert(
                                black_box(order.clone()),
                                &mut balances,
                                &position,
                                init_margin_req,
                            )
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
                    let mut om = OrderMargin::new(NonZeroUsize::new(n).unwrap());
                    let mut balances = Balances::new(QuoteCurrency::new(1_000_000, 0));
                    for order in orders.iter() {
                        om.try_insert(order.clone(), &mut balances, &position, init_margin_req)
                            .expect("Can insert");
                    }
                    (om, balances)
                },
                |(mut order_margin, mut balances)| {
                    for order in orders.iter() {
                        order_margin.fill_order(
                            black_box(order.clone()),
                            &mut balances,
                            &position,
                            init_margin_req,
                        )
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
                    let mut order_margin = OrderMargin::new(NonZeroUsize::new(n).unwrap());
                    let mut balances = Balances::new(QuoteCurrency::new(1_000_000, 0));
                    for order in orders.iter() {
                        order_margin
                            .try_insert(
                                black_box(order.clone()),
                                &mut balances,
                                &position,
                                init_margin_req,
                            )
                            .unwrap()
                    }
                    (order_margin, balances)
                },
                |(mut order_margin, mut balances)| {
                    for order in orders.iter() {
                        order_margin
                            .remove(
                                black_box(CancelBy::OrderId(order.id())),
                                &mut balances,
                                &position,
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
            let position = Position::Neutral;
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut order_margin = OrderMargin::new(NonZeroUsize::new(n).unwrap());
                    let mut balances = Balances::new(QuoteCurrency::new(1_000_000, 0));
                    for order in orders.iter() {
                        order_margin
                            .try_insert(
                                black_box(order.clone()),
                                &mut balances,
                                &position,
                                init_margin_req,
                            )
                            .unwrap()
                    }
                    order_margin
                },
                |order_margin| {
                    let _ = black_box(order_margin.order_margin(init_margin_req, &position));
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_long", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            let position = Position::Long(PositionInner::new(
                BaseCurrency::new(2, 0),
                QuoteCurrency::new(100, 0),
            ));
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut order_margin = OrderMargin::new(NonZeroUsize::new(n).unwrap());
                    let mut balances = Balances::new(QuoteCurrency::new(1_000_000, 0));
                    for order in orders.iter() {
                        order_margin
                            .try_insert(
                                black_box(order.clone()),
                                &mut balances,
                                &position,
                                init_margin_req,
                            )
                            .unwrap()
                    }
                    order_margin
                },
                |order_margin| {
                    let _ = black_box(order_margin.order_margin(init_margin_req, &position));
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("order_margin_short", n), &n, |b, _n| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            let position = Position::Short(PositionInner::new(
                BaseCurrency::new(2, 0),
                QuoteCurrency::new(100, 0),
            ));
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut order_margin = OrderMargin::new(NonZeroUsize::new(n).unwrap());
                    let mut balances = Balances::new(QuoteCurrency::new(1_000_000, 0));
                    for order in orders.iter() {
                        order_margin
                            .try_insert(
                                black_box(order.clone()),
                                &mut balances,
                                &position,
                                init_margin_req,
                            )
                            .unwrap()
                    }
                    order_margin
                },
                |order_margin| {
                    let _ = black_box(order_margin.order_margin(init_margin_req, &position));
                },
            )
        });
    }
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
