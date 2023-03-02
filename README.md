# Leveraged Futures Exchange for Simulated Trading (LFEST)
:warning: This is a personal project, use a your own risk. 

:warning: The results may not represent real trading results on any given exchange. 

lfest-rs is a simulated futures exchange capable of leveraged positions.
 It gets fed external bid ask data to update the internal state
  and check for order execution. For simplicity's sake (and performance) the exchange does not use an order book.
  Supported futures types are both linear and inverse futures.

### Features:
Some of the most notable features include:
- Fixed point arithmetic using [fpdec](https://github.com/mamrhein/fpdec.rs) crate, which is a super fast implementation
- Use of [newtype pattern](https://doc.rust-lang.org/book/ch19-04-advanced-types.html) to enforce the correct function IO.
Examples include `BaseCurrency`, `QuoteCurrency`, `Fee` and `Leverage`. 
This makes it impossible to mistakenly input for example a `USD` denoted value into a function that expects a `BTC` denoted value.
- Flexible market data integration through the `MarketUpdate` type and associated macros.
- Integrated performance tracking. Use the existing `FullTrack` or implement your own using the `AccountTracker` trait.
- Broad test coverage, to get closer to ensured correctness.
- Auditable due to its small and consice codebase.

### Order Types
The supported order types are:
- market        - aggressively execute against the best bid / ask
- limit         - passively place an order into the orderbook

### Performance Metrics:
The following performance metrics are available through AccTracker struct:
- win_ratio
- profit_loss_ratio
- total_rpnl
- sharpe
- sortino
- cumulative fees
- max_drawdown_wallet_balance
- max_drawdown_total
- num_trades
- turnover
- trade_percentage
- buy_ratio
- limit_order_fill_ratio
- limit_order_cancellation_ratio
- historical_value_at_risk
- cornish_fisher_value_at_risk
- d_ratio

Some of these metric may behave differently from what you would expect, so make sure to take a look at the code.

### How to use
To use this crate in your project, add the following to your Cargo.toml:
```
[dependencies]
lfest = "0.29.0"
```

Then proceed to use it in your code.
For an example see [examples](examples/basic.rs)

### TODOs:
- proper liquidations
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
