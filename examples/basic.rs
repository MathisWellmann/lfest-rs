//! Example usage of Exchange using external trade data.
//! A randomly acting agent places market buy / sell orders every 100 candles

use std::{num::NonZeroUsize, time::Instant};

use const_decimal::Decimal;
use lfest::{load_trades_from_csv, prelude::*};
use rand::{Rng, rng};
use tracing::error;

const DECIMALS: u8 = 4;

fn main() {
    let t0 = Instant::now();

    let starting_balance = BaseCurrency::new(10, 0);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 1),
            Decimal::try_from_scaled(2, 0).unwrap(),
            Decimal::zero(),
        )
        .expect("is valid price filter"),
        QuantityFilter::default(),
        Fee::from(Decimal::try_from_scaled(2, 4).unwrap()),
        Fee::from(Decimal::try_from_scaled(6, 4).unwrap()),
    )
    .expect("is valid");
    let config = Config::new(
        starting_balance,
        NonZeroUsize::new(200).unwrap(),
        contract_spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    let mut exchange =
        Exchange::<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId>::new(config);

    // load trades from csv file
    let prices = Vec::from_iter(
        load_trades_from_csv::<i64, DECIMALS>("./data/Bitmex_XBTUSD_1M.csv")
            .iter()
            .map(|t| t.price),
    );

    // use random action every 100 trades to buy or sell
    let mut rng = rng();

    for (i, p) in prices.into_iter().enumerate() {
        let spread = Decimal::try_from_scaled(1, 1).unwrap();
        let exec_orders = exchange
            .update_state(&Bba {
                bid: p,
                ask: p + spread.into(),
                timestamp_exchange_ns: (i as i64).into(),
            })
            .expect("Got REKT. Try again next time :D");
        if !exec_orders.is_empty() {
            println!("executed orders: {:?}", exec_orders);
        }

        if i % 100 == 0 {
            // Trade a fraction of the available wallet balance
            let order_value =
                exchange.balances().available() * Decimal::try_from_scaled(1, 1).unwrap();
            let order_size =
                QuoteCurrency::convert_from(order_value, exchange.market_state().bid());
            let order = if rng.random() {
                MarketOrder::new(Side::Sell, order_size).unwrap() // Sell using
            // market order
            } else {
                MarketOrder::new(Side::Buy, order_size).unwrap() // Buy using market order
            };
            // Handle order error here if needed
            match exchange.submit_market_order(order) {
                Ok(order) => println!("succesfully submitted order: {:?}", order),
                Err(order_err) => error!("an error has occurred: {}", order_err),
            }
        }
    }
    println!(
        "time to simulate 1 million historical trades: {}micros",
        t0.elapsed().as_micros()
    );
}
