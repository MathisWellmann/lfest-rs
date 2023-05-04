//! A risk engine is an essential component of a futures exchange
//! that helps to manage and mitigate risks associated with trading futures contracts.
//! The risk engine is responsible for handling a range of functions related to risk management, including the following:
//!
//! 1. Margin Requirements:
//! The risk engine calculates and monitors margin requirements for each futures contract.
//! Margin is a deposit that traders are required to maintain to cover potential losses in case the price of the underlying asset moves against their position.
//! The risk engine calculates the initial margin required to enter into a position
//! and then monitors the margin requirements on a real-time basis to ensure that they are met.
//!
//! 2. Position Limits:
//! The risk engine enforces position limits on each futures contract to prevent excessive speculation and manipulation of prices.
//! Position limits are set by the exchange and restrict the maximum number of contracts that any trader can hold for a particular futures contract.

use crate::{
    contract_specification::ContractSpecification,
    prelude::Account,
    types::{Currency, MarginCurrency},
};

pub(crate) struct InitialMargin<M>(M);

pub(crate) struct MaintenanceMargin<M>(M);

/// The error that the `RiskEngine` outputs, if any.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RiskError {
    #[error("The `Trader` does not have enough balance.")]
    NotEnoughAvailableBalance,
}

pub(crate) trait RiskEngine<M>
where
    M: Currency + MarginCurrency,
{
    fn check_required_margin(
        &self,
        trader: &Account<M::PairedCurrency>,
        notional_value: M,
    ) -> Result<(InitialMargin<M>, MaintenanceMargin<M>), RiskError>;
}

#[derive(Debug, Clone)]
pub(crate) struct IsolatedMarginRiskEngine<M>
where
    M: Currency + MarginCurrency,
{
    contract_spec: ContractSpecification<M::PairedCurrency>,
}

impl<M> IsolatedMarginRiskEngine<M>
where
    M: Currency + MarginCurrency,
{
    pub(crate) fn new(contract_spec: ContractSpecification<M::PairedCurrency>) -> Self {
        Self { contract_spec }
    }
}

impl<M> RiskEngine<M> for IsolatedMarginRiskEngine<M>
where
    M: Currency + MarginCurrency,
{
    fn check_required_margin(
        &self,
        trader: &Account<M::PairedCurrency>,
        notional_value: M,
    ) -> Result<(InitialMargin<M>, MaintenanceMargin<M>), RiskError> {
        todo!()
    }
}
