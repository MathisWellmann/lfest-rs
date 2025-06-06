//! Compare the crates providing decimal implementations.
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use fpdec::{Dec, Decimal};
use lfest::prelude::*;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("decimal_comparison");

    group.bench_function(&format!("fpdec_quote_convert_from"), |b| {
        b.iter(|| {
            let qty = Dec!(5);
            let price = Dec!(100);
            let _ = black_box(qty * price);
        })
    });
    group.bench_function(&format!("const_decimal_i32_quote_convert_from"), |b| {
        b.iter(|| {
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(5, 0).unwrap();
            let price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let _ = black_box(qty * price);
        })
    });
    group.bench_function(&format!("fpdec_base_convert_from"), |b| {
        b.iter(|| {
            let qty = Dec!(500);
            let price = Dec!(100);
            let _ = black_box(qty / price);
        })
    });
    group.bench_function(&format!("const_decimal_i32_base_convert_from"), |b| {
        b.iter(|| {
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(5, 0).unwrap();
            let price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let _ = black_box(qty / price);
        })
    });
    group.bench_function(&format!("fpdec_linear_futures_pnl"), |b| {
        b.iter(|| {
            let entry_price = Dec!(100);
            let exit_price = Dec!(110);
            let qty = Dec!(5);
            let _ = black_box(exit_price * qty - entry_price * qty);
        })
    });
    group.bench_function(&format!("const_decimal_i32_linear_futures_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(110, 0).unwrap();
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(5, 0).unwrap();
            let _pnl = black_box(exit_price * qty - entry_price * qty);
        })
    });
    group.bench_function(&format!("const_decimal_i64_linear_futures_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i64, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i64, 2>::try_from_scaled(100, 0).unwrap();
            let qty = const_decimal::Decimal::<i64, 2>::try_from_scaled(5, 0).unwrap();
            let _pnl = black_box(exit_price * qty - entry_price * qty);
        })
    });
    group.bench_function(&format!("fpdec_inverse_futures_pnl"), |b| {
        b.iter(|| {
            let entry_price = Dec!(100);
            let exit_price = Dec!(110);
            let qty = Dec!(500);
            let _ = black_box(qty / entry_price - qty / exit_price);
        })
    });
    group.bench_function(&format!("const_decimal_i32_inverse_futures_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(110, 0).unwrap();
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(500, 0).unwrap();
            let _pnl = black_box(qty / entry_price - qty / exit_price);
        })
    });
    group.bench_function(&format!("const_decimal_i64_inverse_futures_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i64, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i64, 2>::try_from_scaled(110, 0).unwrap();
            let qty = const_decimal::Decimal::<i64, 2>::try_from_scaled(500, 0).unwrap();
            let _pnl = black_box(qty / entry_price - qty / exit_price);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
