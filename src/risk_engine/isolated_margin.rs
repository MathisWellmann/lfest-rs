use super::{risk_engine_trait::RiskError, RiskEngine};
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    prelude::Account,
    types::{Currency, MarginCurrency, Order, OrderType},
};

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
        market_state: &MarketState,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
    ) -> Result<M, RiskError> {
        match order.order_type() {
            OrderType::Market => todo!(),
            OrderType::Limit => todo!(),
        }
    }

    fn check_maintenance_margin(
        &self,
        market_state: &MarketState,
        account: &Account<M>,
    ) -> Result<(), RiskError> {
        let pos_value = account.position().size().convert(market_state.mid_price());
        if pos_value < account.position().position_margin() {
            return Err(RiskError::Liquidate);
        }

        Ok(())
    }
}
