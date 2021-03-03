//! Example showing how to load trades from csv file

use std::fs::File;
use std::time::Instant;
use trade_aggregation::Trade;

/// Loads trades from a csv file and on success returns a vector of trades in the proper format
pub fn load_trades_from_csv(filename: &str) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
    let f = File::open(filename)?;

    let mut r = csv::Reader::from_reader(f);

    let mut out: Vec<Trade> = vec![];
    for record in r.records() {
        let row = record?;

        let ts = row[0].parse::<i64>()?;
        let price = row[1].parse::<f64>()?;
        let size = row[2].parse::<f64>()?;

        let trade = Trade {
            timestamp: ts,
            price,
            size,
        };
        out.push(trade);
    }
    Ok(out)
}

fn main() {
    let t0 = Instant::now();
    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();
    println!("last trades: {:?}", trades[trades.len() - 1]);
    println!(
        "loaded {} trades in {}ms",
        trades.len(),
        t0.elapsed().as_millis()
    );
}
