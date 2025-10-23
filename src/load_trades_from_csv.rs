use const_decimal::Decimal;
use num::Zero;

use crate::{
    prelude::Trade,
    types::{
        Mon,
        QuoteCurrency,
        Side,
    },
};

const TIMESTAMP_COL: usize = 0;
const PRICE_COL: usize = 1;
const SIZE_COL: usize = 2;

/// Load trades from csv file. Used only for testing and benchmarking.
///
/// # Arguments:
/// filename: The path to the csv file
pub fn load_trades_from_csv<I, const D: u8>(filename: &str) -> Vec<Trade<I, D, QuoteCurrency<I, D>>>
where
    I: Mon<D>,
{
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
            .parse::<Decimal<I, D>>()
            .expect("Can parse price");
        let quantity = row[SIZE_COL]
            .parse::<Decimal<I, D>>()
            .expect("Can parse size");
        debug_assert_ne!(quantity, Decimal::zero());
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
        #[allow(
            clippy::disallowed_methods,
            reason = "Don't know if we have enough capacity"
        )]
        out.push(trade);
    }

    out
}
