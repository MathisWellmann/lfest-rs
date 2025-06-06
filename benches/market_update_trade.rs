//! Benchmark the `update_state` method of `Exchange` for `TradeEvent`
use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{Criterion, criterion_group, criterion_main};
use lfest::{load_trades_from_csv, prelude::*};

const DECIMALS: u8 = 5;

fn generate_quotes_from_trades(
    trades: &[Trade<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>],
) -> Vec<Bba<i64, DECIMALS>> {
    Vec::from_iter(trades.iter().map(|trade| Bba {
        bid: trade.price - QuoteCurrency::one(),
        ask: trade.price + QuoteCurrency::one(),
        timestamp_exchange_ns: trade.timestamp_exchange_ns,
    }))
}

fn update_state<I, const D: u8, BaseOrQuote, U>(
    exchange: &mut Exchange<I, D, BaseOrQuote, NoUserOrderId>,
    trades: &[U],
) where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    U: MarketUpdate<I, D, BaseOrQuote> + Clone,
{
    for trade in trades.iter() {
        exchange.update_state(trade).expect("is a valid update");
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let starting_balance = BaseCurrency::new(1, 0);
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
        QuantityFilter::new(None, None, QuoteCurrency::one()).expect("is valid filter"),
        Fee::from(Decimal::try_from_scaled(2, 0).unwrap()),
        Fee::from(Decimal::try_from_scaled(6, 0).unwrap()),
    )
    .expect("works");
    let config = Config::new(
        starting_balance,
        200,
        contract_spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    let mut exchange = Exchange::new(config);

    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv");
    const COUNT: usize = 1_000_000;
    assert_eq!(trades.len(), COUNT);

    let mut group = c.benchmark_group("update_state");
    group.throughput(criterion::Throughput::Elements(COUNT as u64));
    group.bench_function("trades_1_million", |b| {
        b.iter(|| {
            update_state::<
                _,
                DECIMALS,
                QuoteCurrency<_, DECIMALS>,
                Trade<_, DECIMALS, QuoteCurrency<_, DECIMALS>>,
            >(black_box(&mut exchange), black_box(&trades))
        })
    });
    let bbas = generate_quotes_from_trades(&trades);
    group.bench_function("quotes_1_million", |b| {
        b.iter(|| {
            update_state::<_, DECIMALS, QuoteCurrency<_, DECIMALS>, Bba<_, DECIMALS>>(
                black_box(&mut exchange),
                black_box(&bbas),
            )
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
