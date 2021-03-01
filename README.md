# Leveraged Futures Exchange for Simulated Trading (LFEST)
:warning: This is a personal project, use a your own risk. 

:warning: The results may not represent real trading results on any given exchange. 

lfest-rs is a blazingly fast simulated exchange capable of leveraged positions.
It gets fed external data either as a trade or a candle to update the internal state
 and check for order execution. For simplicity's sake (and performance) the exchange does not use an order book
 
 
### Order Types
The supported order types are:
- market        - aggressively execute against the best bid / ask
- limit         - passively place an order into the orderbook
- stop_market   - A protective but aggressive market order which is triggered at a specific price 

### External data types
To use raw trade data to update the exchanges state, the Trade struct 
from [trade-aggregation-rs](https://github.com/MathisWellmann/trade_aggregation-rs)
is used.
Each data point must have the following fields:
```rust
/// Defines a taker trade
pub struct Trade {
    /// Timestamp usually denoted in milliseconds
    pub timestamp: i64,
    /// Price of the asset
    pub price: f64,
    /// Size of the trade denoted in QUOTE currency
    /// negative values indicate a taker Sell order
    pub size: f64,
}
```
When constructing the Trade struct pay careful attention to have a timestamp in milliseconds
and a size denoted in QUOTE currency. Sell taker trades have a negative size.
Then, update exchange state using consume_trade(&trade) method.

To use candle data to update the exchange state, the Candle struct 
from [trade-aggregation-rs](https://github.com/MathisWellmann/trade_aggregation-rs)
is used.
To aggregate candles from a &Vec<Trade>:
```rust
// aggregate candles using a volume threshold of 100 BTC, therefore By::Base is used
let mut aggregator = VolumeAggregator::new(100.0, By::Base);
let candles = aggregate_all_trades(&trades, &mut aggregator);
```
Use exchange.consume_candle(&candle) method to update the exchanges state

### Performance Metrics:
The following performance metrics are available through AccTracker struct:
- win_ratio
- profit_loss_ratio
- total_rpnl
- sharpe
- sharpe_daily_returns
- sortino
- cumulative fees
- sharpe_sterling_ratio
- max_drawdown
- max_upnl_drawdown
- num_trades
- turnover
- trade_percentage
- buy_ratio
- limit_order_fill_ratio
- limit_order_cancellation_ratio

### How to use
To use this crate in your project, add the following to your Cargo.toml:
```
[dependencies]
lfest = "0.4.5
```

Then proceed to use it in your code.

The following example uses trades to update the bid and ask price of the exchange.
See [examples](/examples) folder for all the examples. 
Run the following example using:
```shell script
cargo run --example use_trades --release
```

```rust
//! Example usage of Exchange using external trade data.
//! A randomly acting agent places market buy / sell orders every 100 candles

mod load_trades;

#[macro_use]
extern crate log;

use lfest::{Config, Exchange, Order, Side, OrderError};
use load_trades::load_trades_from_csv;
use rand::{thread_rng, Rng};
use std::time::Instant;

fn main() {
    let t0 = Instant::now();

    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.001,
        starting_balance_base: 1.0,
        use_candles: false,
        leverage: 1.0,
    };
    let mut exchange = Exchange::new(config);

    // load trades from csv file
    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // use random action every 100 trades to buy or sell
    let mut rng = thread_rng();

    for (i, t) in trades.iter().enumerate() {
        let liq = exchange.consume_trade(t);
        if liq {
            println!(
                "position liquidated, \
            but there could still be enough wallet_balance to open a new position"
            );
        }

        if i % 100 == 0 {
            // randomly buy or sell using a market order
            let r = rng.gen::<f64>();
            // Trade a fraction of the available wallet balance
            let order_size: f64 = exchange.margin().wallet_balance() * 0.1;
            let order: Order = if r > 0.5 {
                Order::market(Side::Sell, order_size) // Sell using market order
            } else {
                Order::market(Side::Buy, order_size) // Buy using market order
            };
            // Handle order error here if needed
            let response: Result<Order, OrderError> = exchange.submit_order(order);
            match response {
                Ok(order) => println!("succesfully submitted order: {:?}", order),
                Err(order_err) => match order_err {
                    OrderError::MaxActiveOrders => error!("maximum number of active orders reached"),
                    OrderError::InvalidLimitPrice => error!("invalid limit price of order"),
                    OrderError::InvalidTriggerPrice => error!("invalid trigger price of order"),
                    OrderError::InvalidOrderSize => error!("invalid order size"),
                    OrderError::NotEnoughAvailableBalance => error!("not enough available balance in account"),

                }
            }
        }
    }
    println!(
        "time to simulate 1 million historical trades and {} orders: {}ms",
        exchange.acc_tracker().num_trades(),
        t0.elapsed().as_millis()
    );
    analyze_results(&exchange);
}

/// analyze the resulting performance metrics of the traded orders
fn analyze_results(e: &Exchange) {
    let win_ratio = e.acc_tracker().win_ratio();
    let profit_loss_ratio = e.acc_tracker().profit_loss_ratio();
    let rpnl = e.acc_tracker().total_rpnl();
    let sharpe = e.acc_tracker().sharpe();
    let sortino = e.acc_tracker().sortino();
    let sterling_ratio = e.acc_tracker().sharpe_sterling_ratio();
    let max_drawdown = e.acc_tracker().max_drawdown();
    let max_upnl_drawdown = e.acc_tracker().max_upnl_drawdown();
    let num_trades = e.acc_tracker().num_trades();
    let turnover = e.acc_tracker().turnover();
    let buy_ratio = e.acc_tracker().buy_ratio();
    println!("win_ratio: {:.2}, profit_loss_ratio: {:.2}, rpnl: {:.2}, sharpe: {:.2}, sortino: {:.2}, sr: {:.2}, \
    dd: {:.2}, upnl_dd: {:.2}, #trades: {}, turnover: {}, buy_ratio: {:.2},",
        win_ratio,
        profit_loss_ratio,
        rpnl,
        sharpe,
        sortino,
        sterling_ratio,
        max_drawdown,
        max_upnl_drawdown,
        num_trades,
        turnover,
        buy_ratio,
    );
}
```

### Benchmark
See the example use_trades.rs and compile in release mode to see that the Exchange
is capable of simulating 1 million historical trades and executing ~40k market orders in ~470ms.

### Dependencies
A non trivial dependency is [trade_aggregation](https://github.com/MathisWellmann/) 
as the exchange relies on the Trade and Candle struct.

### Contributions
If you find a bug or would like to help out, feel free to create a pull-request.

### Donations :moneybag: :money_with_wings:
I you would like to support the development of this crate, feel free to send over a donation:

Monero (XMR) address:
```plain
47xMvxNKsCKMt2owkDuN1Bci2KMiqGrAFCQFSLijWLs49ua67222Wu3LZryyopDVPYgYmAnYkSZSz9ZW2buaDwdyKTWGwwb
```

![monero](img/monero_donations_qrcode.png)

### License
Copyright (C) 2020  <Mathis Wellmann wellmannmathis@gmail.com>

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.

![GNU AGPLv3](img/agplv3.png)
