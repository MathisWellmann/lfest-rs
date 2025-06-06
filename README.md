# Leveraged Futures Exchange for Simulated Trading (LFEST)

`lfest-rs` is a simulated perpetual futures exchange capable of leveraged positions.
Its optimizes correctness, performance and simplicity in that order.
It can simulate more than 350M trade and 1250M BBa updates per second along with 25M limit order submissions per second (Run `cargo bench` to see it for your system).
You feed in external market data using `Bba`, `Trade`, `Candle` or `SmartCandle` to update the `MarketState`, 
which triggers limit order executions when appropriate.
The exchange can be configured using `Config` and `ContractSpecification`.

:radioactive: This is a personal project, use a your own risk. :bangbang:

### Features:
- :currency_exchange: Fixed point arithmetic using [`const-decimal`](https://github.com/OliverNChalk/const-decimal) crate, for super fast and precise numeric calculations.
- :brain: Use of [newtype pattern](https://doc.rust-lang.org/book/ch19-04-advanced-types.html) to enforce the correct types at function boundaries.
This makes it impossible to mistakenly input for example a `USD` denoted value into a function that expects a `BTC` denoted value. E.g.:
  [`BaseCurrency`](https://docs.rs/lfest/latest/lfest/prelude/struct.BaseCurrency.html),   
  [`QuoteCurrency`](https://docs.rs/lfest/latest/lfest/prelude/struct.QuoteCurrency.html),   
  [`Fee`](https://docs.rs/lfest/latest/lfest/prelude/struct.Fee.html),    
  [`Leverage`](https://docs.rs/lfest/latest/lfest/prelude/struct.Leverage.html).      
- :satellite: Flexible market data integration through the [`MarketUpdate`](https://docs.rs/lfest/latest/lfest/prelude/enum.MarketUpdate.html) trait.
- :heavy_check_mark: good test coverage and heavy use of assertions, to ensure correctness.
- :page_with_curl: Supports both `linear` and `inverse` futures contracts, 
by simply setting the margin currency to either `QuoteCurrency` (linear) or `BaseCurrency` (inverse)
- :no_entry: Order filtering to make sure the price and quantity follow certain rules. See:    
  [`PriceFilter`](https://docs.rs/lfest/latest/lfest/prelude/struct.PriceFilter.html)     
  [`QuantityFilter`](https://docs.rs/lfest/latest/lfest/prelude/struct.QuantityFilter.html)    
- `IsolatedMarginRiskEngine`
- Rate limiting for order submissions, cancellations.

### Order Types
The supported order types are:
- `LimitOrder`: passively place an order into the orderbook, with support for partial executions.
- `MarketOrder`: aggressively execute against the best bid / ask. Not accounting for available volume or full order book for now.

### How to use
To use this crate in your project, add the following to your Cargo.toml:
```ignore
[dependencies]
lfest = { git = https://github.com/MathisWellmann/lfest-rs } # Probably pin the version because main is changing quickly
```

Then proceed to use it in your code.
For an example see [examples](examples/basic.rs)

### TODOs:
- Market orders don't account for slippage and are executed against the best bid and ask without regard for the quantity.
- Funding rate (support `settle_funding_period` in `ClearingHouse`)
- Support for updating leverage on the fly.

### Contributions
Would love to see you use and contribute to this project. Even just adding more tests is welcome.

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
