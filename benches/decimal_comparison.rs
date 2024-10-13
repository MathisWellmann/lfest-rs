use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lfest::prelude::*;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("decimal_comparison");

    group.bench_function(&format!("fpdec_quote_convert_from"), |b| {
        b.iter(|| {
            let qty = Monies::<Decimal, Base>::new(Dec!(5));
            let price = Monies::<Decimal, Quote>::new(Dec!(100));
            let _ = black_box(Quote::convert_from(qty, price));
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
            let qty = Monies::<Decimal, Quote>::new(Dec!(500));
            let price = Monies::<Decimal, Quote>::new(Dec!(100));
            let _ = black_box(Base::convert_from(qty, price));
        })
    });
    group.bench_function(&format!("const_decimal_i32_base_convert_from"), |b| {
        b.iter(|| {
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(5, 0).unwrap();
            let price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let _ = black_box(qty / price);
        })
    });
    group.bench_function(&format!("fpdec_quote_pnl"), |b| {
        b.iter(|| {
            let entry_price = Monies::<Decimal, Quote>::new(Dec!(100));
            let exit_price = Monies::<Decimal, Quote>::new(Dec!(110));
            let qty = Monies::<Decimal, Base>::new(Dec!(5));
            let _ = black_box(Quote::pnl(entry_price, exit_price, qty));
        })
    });
    group.bench_function(&format!("const_decimal_i32_quote_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(110, 0).unwrap();
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(5, 0).unwrap();
            let _pnl = black_box(exit_price * qty - entry_price * qty);
        })
    });
    group.bench_function(&format!("const_decimal_i64_quote_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i64, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i64, 2>::try_from_scaled(100, 0).unwrap();
            let qty = const_decimal::Decimal::<i64, 2>::try_from_scaled(5, 0).unwrap();
            let _pnl = black_box(exit_price * qty - entry_price * qty);
        })
    });
    group.bench_function(&format!("fpdec_base_pnl"), |b| {
        b.iter(|| {
            let entry_price = Monies::<Decimal, Quote>::new(Dec!(100));
            let exit_price = Monies::<Decimal, Quote>::new(Dec!(110));
            let qty = Monies::<Decimal, Quote>::new(Dec!(500));
            let _ = black_box(Base::pnl(entry_price, exit_price, qty));
        })
    });
    group.bench_function(&format!("const_decimal_i32_base_pnl"), |b| {
        b.iter(|| {
            let entry_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(100, 0).unwrap();
            let exit_price = const_decimal::Decimal::<i32, 2>::try_from_scaled(110, 0).unwrap();
            let qty = const_decimal::Decimal::<i32, 2>::try_from_scaled(500, 0).unwrap();
            let _pnl = black_box(qty / entry_price - qty / exit_price);
        })
    });
    group.bench_function(&format!("const_decimal_i64_base_pnl"), |b| {
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
