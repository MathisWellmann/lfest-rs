use super::{risk_engine_trait::RiskError, RiskEngine};
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    prelude::Account,
    types::{Currency, MarginCurrency, Order, OrderType, QuoteCurrency, Side},
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
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
        fill_price: QuoteCurrency,
    ) -> Result<M, RiskError> {
        match order.order_type() {
            OrderType::Market => self.handle_market_order(account, order, fill_price),
            OrderType::Limit => self.handle_limit_order(account, order),
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

impl<M> IsolatedMarginRiskEngine<M>
where
    M: Currency + MarginCurrency,
{
    fn handle_market_order(
        &self,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
        fill_price: QuoteCurrency,
    ) -> Result<M, RiskError> {
        match order.side() {
            Side::Buy => self.handle_market_buy_order(account, order, fill_price),
            Side::Sell => self.handle_market_sell_order(account, order, fill_price),
        }
    }

    fn handle_market_buy_order(
        &self,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
        fill_price: QuoteCurrency,
    ) -> Result<M, RiskError> {
        if account.position.size() >= M::PairedCurrency::new_zero() {
            let notional_value = order.quantity().convert(fill_price);
            let margin_req = notional_value / account.position.leverage;
            let fee = notional_value * self.contract_spec.fee_taker;
            if margin_req + fee > account.available_balance() {
                return Err(RiskError::NotEnoughAvailableBalance);
            }
            return Ok(margin_req);
        }
        // Else its a short position which needs to be reduced
        if order.quantity().into_negative() <= account.position.size() {
            // The order strictly reduces the position, so no additional margin is required.
            return Ok(M::new_zero());
        }
        todo!("The order reduces the short and puts on a long")
    }

    fn handle_market_sell_order(
        &self,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
        fill_price: QuoteCurrency,
    ) -> Result<M, RiskError> {
        if account.position.size() <= M::PairedCurrency::new_zero() {
            let notional_value = order.quantity().convert(fill_price);
            let margin_req = notional_value / account.position.leverage;
            let fee = notional_value * self.contract_spec.fee_taker;
            if margin_req + fee > account.available_balance() {
                return Err(RiskError::NotEnoughAvailableBalance);
            }
            return Ok(margin_req);
        }
        // Else its a long position which needs to be reduced
        if order.quantity() <= account.position.size() {
            // The order strictly reduces the position, so no additional margin is required.
            return Ok(M::new_zero());
        }
        todo!("handle_market_sell_order")
    }

    fn handle_limit_order(
        &self,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
    ) -> Result<M, RiskError> {
        match order.side() {
            Side::Buy => self.handle_limit_buy_order(account, order),
            Side::Sell => self.handle_limit_sell_order(account, order),
        }
    }

    fn handle_limit_buy_order(
        &self,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
    ) -> Result<M, RiskError> {
        todo!("handle_limit_buy_order")
    }

    fn handle_limit_sell_order(
        &self,
        account: &Account<M>,
        order: &Order<M::PairedCurrency>,
    ) -> Result<M, RiskError> {
        todo!("handle_limit_sell_order")
    }
}