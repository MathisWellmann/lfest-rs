//! Configuration types for [`crate::portfolio::PortfolioExchange`].

use std::{
    collections::HashSet,
    num::NonZeroU16,
};

use getset::{
    CopyGetters,
    Getters,
};

use crate::{
    contract_specification::ContractSpecification,
    prelude::{
        ConfigError,
        MarginCurrency,
        Mon,
    },
    types::{
        MarketId,
        OrderRateLimits,
    },
};

/// Configuration for a single market inside a [`crate::portfolio::PortfolioExchange`].
#[derive(Debug, Clone, Getters, CopyGetters)]
pub struct MarketConfig<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Maximum number of open limit orders per side on this market.
    #[getset(get_copy = "pub")]
    max_num_open_orders: NonZeroU16,

    /// Contract specification for this market (ticker, leverage, margin tiers, filters, fees).
    #[getset(get = "pub")]
    contract_spec: ContractSpecification<I, D, Margin::PairedCurrency>,

    /// Order rate limits for this market.
    #[getset(get = "pub")]
    order_rate_limits: OrderRateLimits,
}

impl<I, const D: u8, Margin> MarketConfig<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Create a new [`MarketConfig`].
    #[must_use]
    pub fn new(
        max_num_open_orders: NonZeroU16,
        contract_spec: ContractSpecification<I, D, Margin::PairedCurrency>,
        order_rate_limits: OrderRateLimits,
    ) -> Self {
        Self {
            max_num_open_orders,
            contract_spec,
            order_rate_limits,
        }
    }
}

/// Top-level configuration for a [`crate::portfolio::PortfolioExchange`].
#[derive(Debug, Clone, Getters, CopyGetters)]
pub struct PortfolioConfig<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Shared starting wallet balance in margin currency.
    #[getset(get_copy = "pub")]
    starting_wallet_balance: Margin,

    /// Initial list of markets (assigned dense `MarketId(0..len)`).
    #[getset(get = "pub")]
    markets: Vec<MarketConfig<I, D, Margin>>,
}

impl<I, const D: u8, Margin> PortfolioConfig<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Create a new [`PortfolioConfig`].
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidStartingBalance`] if `starting_wallet_balance <= 0`,
    /// [`ConfigError::TooManyMarkets`] if there are more than [`MarketId::COUNT`] markets, or
    /// [`ConfigError::DuplicateMarketTicker`] if two markets share a non-empty ticker,
    /// because ticker lookups must stay unambiguous.
    pub fn new(
        starting_wallet_balance: Margin,
        markets: Vec<MarketConfig<I, D, Margin>>,
    ) -> Result<Self, ConfigError> {
        if starting_wallet_balance <= Margin::zero() {
            return Err(ConfigError::InvalidStartingBalance);
        }
        if markets.len() > MarketId::COUNT {
            return Err(ConfigError::TooManyMarkets);
        }
        let mut tickers = HashSet::with_capacity(markets.len());
        for market in &markets {
            let ticker = market.contract_spec().ticker();
            if !ticker.is_empty() && !tickers.insert(ticker.as_str()) {
                return Err(ConfigError::DuplicateMarketTicker);
            }
        }
        Ok(Self {
            starting_wallet_balance,
            markets,
        })
    }
}
