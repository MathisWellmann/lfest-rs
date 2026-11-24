//! Configuration types for [`crate::portfolio::PortfolioExchange`].

use std::num::NonZeroU16;

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
    types::OrderRateLimits,
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
    /// Returns [`ConfigError::InvalidStartingBalance`] if `starting_wallet_balance <= 0`.
    pub fn new(
        starting_wallet_balance: Margin,
        markets: Vec<MarketConfig<I, D, Margin>>,
    ) -> Result<Self, ConfigError> {
        if starting_wallet_balance <= Margin::zero() {
            return Err(ConfigError::InvalidStartingBalance);
        }
        Ok(Self {
            starting_wallet_balance,
            markets,
        })
    }
}
