//! Multi-symbol cross-margin perpetual futures portfolio exchange.
//!
//! [`PortfolioExchange`] manages a dense set of markets indexed by [`crate::types::MarketId`]
//! over a single shared margin wallet ([`crate::account::Balances`]). Unrealized profit and
//! loss from every open leg offsets margin across the entire portfolio, and margin calls /
//! liquidations operate across all legs atomically.

mod portfolio_config;
mod portfolio_exchange;
mod portfolio_types;

pub use portfolio_config::{
    MarketConfig,
    PortfolioConfig,
};
pub use portfolio_exchange::PortfolioExchange;
pub use portfolio_types::{
    DelistSettlement,
    MarketSlotView,
    PendingLimitOrder,
    PortfolioError,
    PortfolioFundingSettlement,
    PortfolioMarketOrderSettlement,
    PortfolioOrderEvent,
    PortfolioStopEvent,
    TaggedForcedCancels,
};
