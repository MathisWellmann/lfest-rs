# Leveraged Futures Exchange for Simulated Trading (LFEST)
:warning: This is a personal project, use a your own risk. 

:warning: The results may not represent real trading results on any given exchange. 

This crate aims to be a high performance simulated exchange capable of leveraged positions.

### Order Types
The supported order types are:
- market,
- limit
- stop_market

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
lfest = { git = "https://github.com/MathisWellmann/lfest-rs" }
```

Then proceed to use it in your code.
The following example uses a Trade to update the bid and ask price of the exchange.

```rust
mod load_trades;

use lfest::{Config, Exchange, Order, Side};
use load_trades::load_trades_from_csv;
use rand::{thread_rng, Rng};
use std::time::Instant;

fn main() {
    let t0 = Instant::now();

    let config = Config{
        fee_maker: -0.00025,
        fee_taker: 0.001,
        starting_balance_base: 1.0,
        use_candles: false,
        leverage: 1.0
    };
    let mut exchange = Exchange::new(config);

    // load trades from csv file
    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // use random action to buy or sell
    let mut rng = thread_rng();

    for t in &trades {
        let liq = exchange.consume_trade(t);
        if liq {
            println!(
                "position liquidated, \
            but there could still be enough wallet_balance to open a new position"
            );
        }

        // randomly buy or sell using a market order
        let r = rng.gen::<f64>();
        // Trade a fraction of the available wallet balance
        let order_size: f64 = exchange.margin().wallet_balance() * 0.01;
        let order: Order = if r > 0.98 {
            // Sell order
            Order::market(Side::Sell, order_size)
        } else if r < 0.02 {
            // BUY
            Order::market(Side::Buy, order_size)
        } else {
            // Neutral
            continue;
        };
        let _order_err = exchange.submit_order(order);
        // Handle order error here if needed
    }
    println!(
        "time to simulate 1 million historical trades and {} orders: {}ms",
        exchange.acc_tracker().num_trades(),
        t0.elapsed().as_millis()
    );
    analyze_results(&exchange);
}

/// analyzer the resulting performance metrics of the traded orders
fn analyze_results(e: &Exchange) {
    let rpnl = e.acc_tracker().total_rpnl();
    let sharpe = e.acc_tracker().sharpe();
    let sortino = e.acc_tracker().sortino();
    let sterling_ratio = e.acc_tracker().sharpe_sterling_ratio();
    let max_drawdown = e.acc_tracker().max_drawdown();
    let max_upnl_drawdown = e.acc_tracker().max_upnl_drawdown();
    let num_trades = e.acc_tracker().num_trades();
    let buy_ratio = e.acc_tracker().buy_ratio();
    let turnover = e.acc_tracker().turnover();
    let win_ratio = e.acc_tracker().win_ratio();
    println!(
        "rpnl: {:.2}, sharpe: {:.2}, sortino: {:.2}, sr: {:.2}, \
    dd: {:.2}, upnl_dd: {:.2}, #trades: {}, buy_ratio: {:.2}, turnover: {}, win_ratio: {}",
        rpnl,
        sharpe,
        sortino,
        sterling_ratio,
        max_drawdown,
        max_upnl_drawdown,
        num_trades,
        buy_ratio,
        turnover,
        win_ratio
    );
}

```
See the examples folder for more code.

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
