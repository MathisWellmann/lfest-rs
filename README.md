# Leveraged Futures Exchange for Simulated Trading (LFEST)
:radioactive: This is a personal project, use a your own risk.   

lfest-rs is a simulated futures exchange capable of leveraged positions.    
You fed it external market data through the [`MarketUpdate`](https://docs.rs/lfest/0.31.0/lfest/prelude/enum.MarketUpdate.html) enum to update the internal state.  
Where you either provide bid and ask price or information derived from a [candle](https://github.com/MathisWellmann/trade_aggregation-rs).   
Macros ([`bba`](https://docs.rs/lfest/0.31.0/lfest/macro.bba.html), [`candle`](https://docs.rs/lfest/0.31.0/lfest/macro.candle.html)) make it easy to construct the concrete variant.   
For simplicity's sake (and performance) the exchange does not use an order book.   
The exchange can be configured using [`Config`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.Config.html)

### Features:
- :currency_exchange: Fixed point arithmetic using [`fpdec`](https://github.com/mamrhein/fpdec.rs) crate, for super fast and precise numeric calculations.
- :brain: Use of [newtype pattern](https://doc.rust-lang.org/book/ch19-04-advanced-types.html) to enforce the correct types at function boundaries.   
Examples include 
[`BaseCurrency`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.BaseCurrency.html), 
[`QuoteCurrency`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.QuoteCurrency.html), 
[`Fee`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.Fee.html) and 
[`Leverage`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.Leverage.html).   
This makes it impossible to mistakenly input for example a `USD` denoted value into a function that expects a `BTC` denoted value.    
- :satellite: Flexible market data integration through the [`MarketUpdate`](https://docs.rs/lfest/0.31.0/lfest/prelude/enum.MarketUpdate.html) type and associated macros.   
- :chart: Integrated performance tracking.    
Use the existing [`FullAccountTracker`](https://docs.rs/lfest/0.31.0/lfest/account_tracker/struct.FullAccountTracker.html)  
or implement your own using the [`AccountTracker`](https://docs.rs/lfest/0.31.0/lfest/account_tracker/trait.AccountTracker.html) trait.
- :heavy_check_mark: Broad test coverage, to get closer to ensured correctness.
- :mag: Auditable due to its small and consice codebase. < 8k LOC
- :page_with_curl: Supports both linear and inverse futures contracts.
- :no_entry: Order filtering to make sure the price and quantity follow certain rules. 
See [`PriceFilter`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.PriceFilter.html) and 
[`QuantityFilter`](https://docs.rs/lfest/0.31.0/lfest/prelude/struct.QuantityFilter.html)

### Order Types
The supported order types are:
- `Market`: aggressively execute against the best bid / ask
- `Limit`: passively place an order into the orderbook

### Performance Metrics:
The following performance metrics are available when using the `FullTrack` `AccountTracker`,   
but you may define any performance metric by implementing the `AccountTracker` trait.
- `win_ratio`: wins / total_trades
- `profit_loss_ratio`: avg_win_amnt / avg_loss_amnt
- `total_rpnl`: Total realized profit and loss
- `sharpe`
- `sortino`
- `cumulative fees`: Sum total of fees payed to the exchange
- `max_drawdown_wallet_balance`: Maximum fraction the wallet balance has decreased from its high.
- `max_drawdown_total`: Drawdown including unrealized profit and loss
- `num_trades`: The total number of trades executed
- `turnover`: The total quantity executed 
- `trade_percentage`: trades / total_trade_opportunities
- `buy_ratio`: buys / total_trades
- `limit_order_fill_ratio`
- `limit_order_cancellation_ratio`
- `historical_value_at_risk`
- `cornish_fisher_value_at_risk`
- `d_ratio`

Some of these metric may behave differently from what you would expect, so make sure to take a look at the code.

### How to use
To use this crate in your project, add the following to your Cargo.toml:
```ignore
[dependencies]
lfest = "0.33.0"
```

Then proceed to use it in your code.
For an example see [examples](examples/basic.rs)

### TODOs:
- proper liquidations
- More modular and testable `AccountTracker`
- Rework some internal components for greater clarity and simplicity

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
