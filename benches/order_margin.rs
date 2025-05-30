//! Benchmark regarding checking of active limit orders.

use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{Criterion, criterion_group, criterion_main};
use lfest::prelude::*;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("OrderMargin");

    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::<i64, 5>::new(99, 0),
        BaseCurrency::new(1, 1),
    )
    .unwrap();

    for n in [1, 5, 10, 100] {
        group.bench_function(&format!("insert_{n}"), |b| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || OrderMargin::new(n),
                |mut order_margin| {
                    for order in orders.iter() {
                        order_margin
                            .try_insert(black_box(order.clone()))
                            .expect("Can insert")
                    }
                },
            )
        });
        group.bench_function(&format!("update_{n}"), |b| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut om = OrderMargin::new(n);
                    for order in orders.iter() {
                        om.try_insert(order.clone()).expect("Can insert");
                    }
                    om
                },
                |mut order_margin| {
                    for order in orders.iter() {
                        order_margin.update(black_box(order.clone()))
                    }
                },
            )
        });
        group.bench_function(&format!("remove_{n}"), |b| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            b.iter_with_setup(
                || {
                    let mut order_margin = OrderMargin::new(n);
                    for order in orders.iter() {
                        order_margin.try_insert(black_box(order.clone())).unwrap()
                    }
                    order_margin
                },
                |mut order_margin| {
                    for order in orders.iter() {
                        order_margin
                            .remove(black_box(CancelBy::OrderId(order.id())))
                            .expect("Can insert");
                    }
                },
            )
        });
        group.bench_function(&format!("order_margin_neutral_{n}"), |b| {
            let orders = Vec::from_iter((0..n).map(|i| {
                let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
                order.clone().into_pending(meta)
            }));
            let position = Position::Neutral;
            let init_margin_req = Decimal::one();
            b.iter_with_setup(
                || {
                    let mut order_margin = OrderMargin::new(n);
                    for order in orders.iter() {
                        order_margin.try_insert(black_box(order.clone())).unwrap()
                    }
                    order_margin
                },
                |order_margin| {
                    let _ = black_box(order_margin.order_margin(init_margin_req, &position));
                },
            )
        });
        group.bench_function(&format!("order_margin_long_{n}"), |b| {
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
                    let mut order_margin = OrderMargin::new(n);
                    for order in orders.iter() {
                        order_margin.try_insert(black_box(order.clone())).unwrap()
                    }
                    order_margin
                },
                |order_margin| {
                    let _ = black_box(order_margin.order_margin(init_margin_req, &position));
                },
            )
        });
        group.bench_function(&format!("order_margin_short_{n}"), |b| {
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
                    let mut order_margin = OrderMargin::new(n);
                    for order in orders.iter() {
                        order_margin.try_insert(black_box(order.clone())).unwrap()
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
