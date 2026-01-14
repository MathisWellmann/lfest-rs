#![doc = include_str!("../README.md")]
#![feature(vec_push_within_capacity)]

//! lfest - leveraged futures exchange for simulated trading

mod active_limit_orders;
mod config;
mod contract_specification;
mod exchange;
mod expect_messages;
mod load_trades_from_csv;
mod market_state;
mod market_update;
mod mock_exchange;
mod order_filters;
mod order_margin;
mod order_rate_limiter;
mod position;
mod position_inner;
mod risk_engine;
#[cfg(test)]
mod tests;
#[cfg(feature = "trade_aggregation")]
mod trade_aggregation;
mod types;
mod utils;

pub use expect_messages::*;
pub use load_trades_from_csv::load_trades_from_csv;
pub use mock_exchange::*;
pub use types::Result;

/// Exports common types
pub mod prelude {
    pub use const_decimal;
    pub use num_traits::{
        One,
        Zero,
    };

    pub use crate::{
        active_limit_orders::ActiveLimitOrders,
        config::Config,
        contract_specification::*,
        exchange::{
            Account,
            Exchange,
        },
        leverage,
        market_state::MarketState,
        market_update::*,
        order_filters::{
            PriceFilter,
            QuantityFilter,
        },
        order_margin::OrderMargin,
        position::Position,
        position_inner::PositionInner,
        types::*,
        utils::{
            NoUserOrderId,
            decimal_from_f64,
            scale,
        },
    };
}
