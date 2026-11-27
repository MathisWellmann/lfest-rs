mod amend;
mod cancel_limit_order;
mod cross_margin;
mod funding;
mod mark_price_and_filters;
mod order_flags_and_tif;
mod partial_order_fill;
mod portfolio;
mod realism_and_ledger;
mod reduce_position_order_margin;
mod stop_orders;
mod submit_limit_buy_order;
mod submit_limit_sell_order;
mod submit_market_buy_order;
mod submit_market_sell_order;
mod update_state;

use crate::{
    DECIMALS,
    prelude::*,
};

/// A best bid and ask update with whole-number prices, shared by the integration tests.
fn bba(bid: i64, ask: i64, ts: i64) -> Bba<i64, DECIMALS> {
    Bba {
        bid: QuoteCurrency::new(bid, 0),
        ask: QuoteCurrency::new(ask, 0),
        timestamp_exchange_ns: ts.into(),
    }
}

#[allow(unused, reason = "Used in benchmarks")]
use criterion::*;
#[allow(unused, reason = "Used in benchmarks")]
use fpdec::*;
