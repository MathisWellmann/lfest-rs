//! Example usage of Exchange using external trade data.
//! A randomly acting agent places market buy / sell orders every 100 candles

mod load_trades;

use std::{convert::TryInto, time::Instant};

use lfest::{account_tracker::FullAccountTracker, prelude::*};
use load_trades::load_prices_from_csv;
use rand::{thread_rng, Rng};
use tracing::error;

fn main() {
    let t0 = Instant::now();

    let starting_balance = base!(10);
    let acc_tracker = FullAccountTracker::new(starting_balance);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter {
            min_price: None,
            max_price: None,
            tick_size: quote!(0.1),
            multiplier_up: Dec!(2),
            multiplier_down: Dec!(0),
        },
        QuantityFilter::default(),
        fee!(0.0002),
        fee!(0.0006),
    )
    .expect("is valid");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    let mut exchange = Exchange::<
        FullAccountTracker<BaseCurrency>,
        QuoteCurrency,
        (),
        InMemoryTransactionAccounting<BaseCurrency>,
    >::new(acc_tracker, config);

    // load trades from csv file
    let prices = load_prices_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // use random action every 100 trades to buy or sell
    let mut rng = thread_rng();

    for (i, p) in prices.iter().enumerate() {
        let price_decimal: Decimal = (*p).try_into().expect("Unable to convert f64 into Decimal");
        let spread: Decimal = Decimal::ONE / Decimal::from(10);
        let exec_orders = exchange
            .update_state(
                (i as i64).into(),
                bba!(
                    QuoteCurrency::new(price_decimal),
                    QuoteCurrency::new(price_decimal + spread)
                ),
            )
            .expect("Got REKT. Try again next time :D");
        if !exec_orders.is_empty() {
            println!("executed orders: {:?}", exec_orders);
        }

        if i % 100 == 0 {
            // Trade a fraction of the available wallet balance
            let order_value: BaseCurrency =
                exchange.user_balances().available_wallet_balance * Dec!(0.1);
            let order_size = order_value.convert(exchange.market_state().bid());
            let order = if rng.gen() {
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
    println!("account_tracker: {}", exchange.account_tracker());
}
