extern crate trade_aggregation;

use std::fs::File;
use trade_aggregation::Trade;

pub fn load_trades_from_csv(filename: &str) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
    let f = File::open(filename)?;

    let mut r = csv::Reader::from_reader(f);

    let mut out: Vec<Trade> = vec![];
    for record in r.records() {
        let row = record?;

        let ts = row[0].parse::<i64>()?;
        let price = row[1].parse::<f64>()?;
        let size = row[2].parse::<f64>()?;

        let trade = Trade{
            timestamp: ts,
            price,
            size,
        };
        out.push(trade);
    }
    Ok(out)
}

fn main() {

}