//! Benchmark the `SmartCandle`

#![allow(
    unused_crate_dependencies,
    missing_docs,
    reason = "Benchmarks don't use all dev-dependencies"
)]

use std::num::NonZeroUsize;

use const_decimal::Decimal;
use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use lfest::{
    load_trades_from_csv,
    prelude::*,
};

type DecimalT = i64;
const DECIMALS: u8 = 1;

fn criterion_benchmark(c: &mut Criterion) {
    let starting_balance = BaseCurrency::<DecimalT, DECIMALS>::new(1, 0);
    let price_filter = PriceFilter::new(
        None,
        None,
        QuoteCurrency::new(5, 1),
        Decimal::TWO,
        Decimal::try_from_scaled(5, 1).unwrap(),
    )
    .expect("is valid filter");
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        price_filter.clone(),
        QuantityFilter::new(None, None, QuoteCurrency::one()).expect("is valid filter"),
        Fee::from(Decimal::try_from_scaled(2, 0).unwrap()),
        Fee::from(Decimal::try_from_scaled(6, 0).unwrap()),
    )
    .expect("works");
    let config = Config::new(
        starting_balance,
        NonZeroUsize::new(200).unwrap(),
        contract_spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    let mut exchange =
        Exchange::<DecimalT, DECIMALS, QuoteCurrency<DecimalT, DECIMALS>, NoUserOrderId>::new(
            config,
        );

    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv");
    const COUNT: usize = 1_000_000;
    assert_eq!(trades.len(), COUNT);

    let candles = Vec::from_iter(trades.chunks(1_000).map(|chunk| {
        let bba = Bba {
            bid: chunk.last().unwrap().price,
            ask: chunk.last().unwrap().price + QuoteCurrency::new(1, 0),
            timestamp_exchange_ns: chunk.last().unwrap().timestamp_exchange_ns(),
        };
        SmartCandle::new(chunk, bba, &price_filter)
    }));
    assert_eq!(candles.len(), 1_000);

    let mut group = c.benchmark_group("SmartCandle");
    group.throughput(criterion::Throughput::Elements(COUNT as u64));
    group.bench_function("update_state", |b| {
        b.iter(|| {
            for trade in candles.iter() {
                exchange.update_state(trade).expect("is a valid update");
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
