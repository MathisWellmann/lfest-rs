//! Example showing how to load trades from csv file

use std::{fs::File, time::Instant};

use const_decimal::Decimal;
use lfest::prelude::Mon;

/// Loads trades from a csv file and on success returns a vector of trades in
/// the proper format
pub fn load_prices_from_csv<I, const D: u8>(
    filename: &str,
) -> Result<Vec<Decimal<I, D>>, Box<dyn std::error::Error>>
where
    I: Mon<D> + 'static,
{
    let f = File::open(filename)?;

    let mut r = csv::Reader::from_reader(f);

    let mut out = Vec::with_capacity(1_000_000);
    for record in r.records() {
        let row = record?;

        let price = row[1].parse()?;

        out.push(price);
    }
    Ok(out)
}

#[allow(dead_code)]
fn main() {
    let t0 = Instant::now();
    let trades = load_prices_from_csv::<i64, 1>("./data/Bitmex_XBTUSD_1M.csv").unwrap();
    println!("last trades: {:?}", trades[trades.len() - 1]);
    println!(
        "loaded {} trades in {}ms",
        trades.len(),
        t0.elapsed().as_millis()
    );
}
