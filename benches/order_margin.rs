//! Benchmark regarding `OrderMargin`

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

mod common;

use std::{
    hint::black_box,
    num::NonZeroU16,
};

use criterion::{
    BenchmarkGroup,
    BenchmarkId,
    Criterion,
    criterion_group,
    criterion_main,
    measurement::WallTime,
};
use lfest::{
    prelude::*,
    test_fee_maker,
};

type ThisBase = BaseCurrency<i64, 5>;

type ThisAccount = Account<i64, 5, ThisBase, NoUserOrderId>;

type ThisOrder = LimitOrder<i64, 5, ThisBase, NoUserOrderId, Pending<i64, 5, ThisBase>>;

/// An account with enough balance to hold every benchmarked order.
fn account(max_active_orders: NonZeroU16) -> ThisAccount {
    Account::new(
        Balances::new(QuoteCurrency::new(1_000_000, 0)),
        max_active_orders,
        const_decimal::Decimal::ONE,
        *test_fee_maker().as_ref(),
    )
}

/// `n` pending limit orders at the same price, with the side chosen by
/// `side_of`.
///
/// The side is what makes the long, short and neutral margin cases differ.
/// Previously all three families inserted buy orders, so all three measured
/// the exact same call on the exact same state.
fn pending_orders(n: usize, side_of: impl Fn(usize) -> Side) -> Vec<ThisOrder> {
    assert!(n <= 1_000, "keeps the account within its margin limit");
    let orders = Vec::from_iter((0..n).map(|i| {
        let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
        LimitOrder::new(
            side_of(i),
            QuoteCurrency::<i64, 5>::new(99, 0),
            BaseCurrency::new(1, 1),
        )
        .expect("order is valid")
        .into_pending(meta)
    }));
    assert_eq!(orders.len(), n, "every order is built");
    orders
}

fn bench_insert(group: &mut BenchmarkGroup<'_, WallTime>, n: usize, max_active_orders: NonZeroU16) {
    let orders = pending_orders(n, |_| Side::Buy);
    group.bench_function(BenchmarkId::new("insert", n), |b| {
        b.iter_with_setup(
            || account(max_active_orders),
            |mut account| {
                for order in orders.iter() {
                    account
                        .try_insert_order(black_box(order.clone()))
                        .expect("Can insert");
                }
            },
        )
    });
}

fn bench_remove(group: &mut BenchmarkGroup<'_, WallTime>, n: usize, max_active_orders: NonZeroU16) {
    let orders = pending_orders(n, |_| Side::Buy);
    group.bench_function(BenchmarkId::new("remove", n), |b| {
        b.iter_with_setup(
            || {
                let mut account = account(max_active_orders);
                for order in orders.iter() {
                    account
                        .try_insert_order(black_box(order.clone()))
                        .expect("Can insert");
                }
                account
            },
            |mut account| {
                for order in orders.iter() {
                    account
                        .cancel_limit_order(black_box(CancelBy::OrderId(order.id())))
                        .expect("Can cancel");
                }
            },
        )
    });
}

fn bench_order_margin(
    group: &mut BenchmarkGroup<'_, WallTime>,
    n: usize,
    max_active_orders: NonZeroU16,
    name: &str,
    side_of: impl Fn(usize) -> Side,
) {
    let orders = pending_orders(n, side_of);
    group.bench_function(BenchmarkId::new(name, n), |b| {
        b.iter_with_setup(
            || {
                let mut account = account(max_active_orders);
                for order in orders.iter() {
                    account
                        .try_insert_order(black_box(order.clone()))
                        .expect("Can insert");
                }
                account
            },
            |account| {
                let _ = black_box(account.order_margin());
            },
        )
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    const MAX_OPEN_ORDERS: u16 = 100;
    let max_active_orders = NonZeroU16::new(MAX_OPEN_ORDERS).expect("100 is not zero");
    let mut group = common::group(c, "OrderMargin");

    for n in common::sweep(usize::from(MAX_OPEN_ORDERS)) {
        bench_insert(&mut group, n, max_active_orders);
        bench_remove(&mut group, n, max_active_orders);
    }

    // `Account::order_margin` sums the cached bid and ask notionals, so it is
    // O(1) in the number of active orders: an N sweep would only measure the
    // same call again. A full book is the realistic worst case.
    let n = usize::from(MAX_OPEN_ORDERS);
    bench_order_margin(
        &mut group,
        n,
        max_active_orders,
        "order_margin_neutral",
        |i| {
            if i % 2 == 0 { Side::Buy } else { Side::Sell }
        },
    );
    bench_order_margin(
        &mut group,
        n,
        max_active_orders,
        "order_margin_long",
        |_| Side::Buy,
    );
    bench_order_margin(
        &mut group,
        n,
        max_active_orders,
        "order_margin_short",
        |_| Side::Sell,
    );
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
