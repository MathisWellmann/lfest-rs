use std::hint::black_box;

use const_decimal::Decimal;
use criterion::{criterion_group, criterion_main, Criterion};
use lfest::prelude::*;

const DECIMALS: u8 = 5;

const TIMESTAMP_COL: usize = 0;
const PRICE_COL: usize = 1;
const SIZE_COL: usize = 2;

/// Load trades from csv file
///
/// # Arguments:
/// filename: The path to the csv file
///
/// # Returns
/// If Ok, A vector of the trades inside the file
fn load_trades_from_csv(filename: &str) -> Vec<Trade<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>> {
    let f = std::fs::File::open(filename).expect("Can open file");

    let mut r = csv::Reader::from_reader(f);

    // Make sure that the header matches what we are trying to parse.
    let head = r.headers().expect("CSV file has a header.");
    assert_eq!(&head[TIMESTAMP_COL], "timestamp");
    assert_eq!(&head[PRICE_COL], "price");
    assert_eq!(&head[SIZE_COL], "size");

    let mut out = Vec::with_capacity(1_000_000);
    for record in r.records() {
        let row = record.expect("Can read record.");

        let ts_ms: i64 = row[TIMESTAMP_COL].parse().expect("Can parse timestamp");
        let price = row[PRICE_COL]
            .parse::<Decimal<i64, DECIMALS>>()
            .expect("Can parse price");
        let quantity = row[SIZE_COL]
            .parse::<Decimal<i64, DECIMALS>>()
            .expect("Can parse size");
        assert_ne!(quantity, Decimal::zero());
        let side = if quantity < Decimal::ZERO {
            Side::Sell
        } else {
            Side::Buy
        };

        // convert to Trade
        let trade = Trade {
            price: QuoteCurrency::from(price),
            quantity: QuoteCurrency::from(quantity),
            side,
            timestamp_exchange_ns: (ts_ms * 1_000_000).into(),
        };
        out.push(trade);
    }

    out
}

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
    exchange: &mut Exchange<
        I,
        D,
        BaseOrQuote,
        (),
        InMemoryTransactionAccounting<I, D, BaseOrQuote::PairedCurrency>,
        NoAccountTracker,
    >,
    trades: &[U],
) where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    U: MarketUpdate<I, D, BaseOrQuote>,
{
    for trade in trades.into_iter() {
        exchange.update_state(trade).expect("is a valid update");
    }
}

// TODO: benchmark for different types other than `Decimal`
fn criterion_benchmark(c: &mut Criterion) {
    let starting_balance = BaseCurrency::new(1, 0);
    let acc_tracker = NoAccountTracker::default();
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
        Position::default(),
        200,
        contract_spec,
        3600,
    )
    .unwrap();
    let mut exchange = Exchange::new(acc_tracker, config);

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
