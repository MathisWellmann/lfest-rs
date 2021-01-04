# Leveraged Futures Exchange for Simulated Trading (LFEST)
:warning: This is a personal project, use a your own risk. 

:warning: The results may not represent real trading results on any given exchange. 

This crate aims to be a high performance simulated exchange capable of leveraged positions.

There are two implementations one with the Decimal type for high accuracy and proper test coverage,
and the other type being f64 for high performance. 
Both need to have the same code in order to ensure the correctness of the f64 implementation.

Currently, no guarantee of feature completeness is given as it is not of high priority for my personal backtesting.

## Example Usage
Add to your Cargo.toml
```
lfest = "0.3.0"
```
Then proceed to use it in your code.
The following example uses a Trade to update the bid and ask price of the exchange.
```rust
mod load_trades;

use lfest::{ExchangeFloat, ConfigFloat, Side, OrderFloat};
use load_trades::load_trades_from_csv;
use rand::{thread_rng, Rng};

fn main() {
    // configure fees. set custom fees in the struct if needed
    let config = ConfigFloat::bitmex_perpetuals();
    let mut exchange = ExchangeFloat::new(config);

    exchange.set_leverage(2.0);

    // load trades from csv file
    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // use random action to buy or sell
    let mut rng = thread_rng();

    for t in &trades {
        let liq = exchange.consume_trade(t);
        if liq {
            println!("position liquidated, \
            but there could still be enough wallet_balance to open a new position");
        }

        // randomly buy or sell using a market order
        let r = rng.gen::<f64>();
        // Trade a fraction of the available wallet balance
        let order_size: f64 = exchange.margin.wallet_balance * 0.01;
        let order: OrderFloat = if r > 0.98 {
            // Sell order
            OrderFloat::market(Side::Sell, order_size)
        } else if r < 0.02 {
            // BUY
            OrderFloat::market(Side::Buy, order_size)
        } else {
            // Neutral
            continue
        };
        let _order_err = exchange.submit_order(order);
        // Handle order error here if needed
    }
}
```
See the examples folder for more code.

## Resulting Performance metrics
All performance metrics can be obtained from the AccTracker struct like so, where e is the exchange:
```rust
let rpnl = e.acc_tracker.total_rpnl();
let sharpe = e.acc_tracker.sharpe();
let sortino = e.acc_tracker.sortino();
let sterling_ratio = e.acc_tracker.sharpe_sterling_ratio();
let max_drawdown = e.acc_tracker.max_drawdown();
let max_upnl_drawdown = e.acc_tracker.max_upnl_drawdown();
let num_trades = e.acc_tracker.num_trades();
let buy_ratio = e.acc_tracker.buy_ratio();
let turnover = e.acc_tracker.turnover();
let win_ratio = e.acc_tracker.win_ratio();
```

## Order Types
There are two order structs one for each different exchange type:
OrderFloat and OrderDecimal. They do however act the same.
Here are the supported order types:
- market(Side, size),
- limit(Side, size, price)
- stop_market
- take_profit_limit
- take_profit_market
Check out the OrderType enum in lib.rs for all the variants.
Only market and limit orders should work properly as the others have not been tested fully yet.

## Benchmark
See the example use_trades.rs and compile in release mode to see that the ExchangeFloat
is capable of simulating 1 million historical trades and executing ~40k market orders in ~470ms.
If you were to use the ExchangeDecimal implementation which uses the Decimal type for high precision, 
the performance drops significantly and it should only be used when high precision is of great importance.

## Dependencies
A non trivial dependency is [trade_aggregation](https://github.com/MathisWellmann/) 
as the exchange relies on the Trade and Candle struct.

## TODOS:
- performance comparison between both exchanges
- publish to crates.io
- possibly remove trade_aggregation dependency
- possibly generic implementation for both Decimal and f64

## Contributions
If you find a bug or would like to help out, feel free to create a pull-request.

## License
This crate is licensed under GNU GPLv3

