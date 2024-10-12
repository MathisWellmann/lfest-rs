use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use fpdec::Decimal;
use lfest::prelude::*;

/// Load trades from csv file
///
/// # Arguments:
/// filename: The path to the csv file
///
/// # Returns
/// If Ok, A vector of the trades inside the file
fn load_trades_from_csv(filename: &str) -> Vec<Trade<Decimal, Quote>> {
    let f = std::fs::File::open(filename).expect("Can open file");

    let mut r = csv::Reader::from_reader(f);

    let mut out: Vec<Trade<Decimal, Quote>> = Vec::new();
    for record in r.records() {
        let row = record.expect("Can read record.");

        let price: Decimal = row[1]
            .parse::<f64>()
            .expect("Can parse price")
            .try_into()
            .expect("Can parse");
        let quantity = row[2].parse::<i32>().expect("Can parse size");
        assert_ne!(quantity, 0);
        let side = if quantity < 0 { Side::Sell } else { Side::Buy };

        // convert to Trade
        let trade = Trade {
            price: QuoteCurrency::new(price),
            quantity: QuoteCurrency::new(Decimal::from(quantity)),
            side,
        };
        out.push(trade);
    }

    out
}

fn generate_quotes_from_trades(trades: &[Trade<Decimal, Quote>]) -> Vec<Bba<Decimal>> {
    Vec::from_iter(trades.iter().map(|v| Bba {
        bid: v.price - QuoteCurrency::new(Dec!(1)),
        ask: v.price + QuoteCurrency::new(Dec!(1)),
    }))
}

fn update_state<T, BaseOrQuote, U>(
    exchange: &mut Exchange<
        NoAccountTracker,
        T,
        BaseOrQuote,
        (),
        InMemoryTransactionAccounting<T, BaseOrQuote::PairedCurrency>,
    >,
    trades: &[U],
) where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
    U: MarketUpdate<T, BaseOrQuote, ()>,
{
    for (i, trade) in trades.into_iter().enumerate() {
        let ts_ns: TimestampNs = (i as i64).into();
        exchange
            .update_state(ts_ns, trade)
            .expect("is a valid update");
    }
}

// TODO: benchmark for different types other than `Decimal`
fn criterion_benchmark(c: &mut Criterion) {
    let starting_balance = base!(1);
    let acc_tracker = NoAccountTracker::default();
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::new(None, None, quote!(0.5), Dec!(2), Dec!(0.5)).expect("is valid filter"),
        QuantityFilter::new(None, None, quote!(1)).expect("is valid filter"),
        Fee::from_basis_points(2),
        Fee::from_basis_points(6),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    let mut exchange = Exchange::new(acc_tracker, config);

    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv");
    const COUNT: usize = 1_000_000;
    assert_eq!(trades.len(), COUNT);

    let mut group = c.benchmark_group("update_state");
    group.throughput(criterion::Throughput::Elements(COUNT as u64));
    group.bench_function("trades_1_million", |b| {
        b.iter(|| {
            update_state::<_, Quote, Trade<Decimal, Quote>>(
                black_box(&mut exchange),
                black_box(&trades),
            )
        })
    });
    let bbas = generate_quotes_from_trades(&trades);
    group.bench_function("quotes_1_million", |b| {
        b.iter(|| {
            update_state::<_, Quote, Bba<Decimal>>(black_box(&mut exchange), black_box(&bbas))
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
