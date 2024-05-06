use tracing::debug;

use super::{risk_engine_trait::RiskError, RiskEngine};
use crate::{
    contract_specification::ContractSpecification,
    fees_of_limit_orders::fees_of_limit_orders,
    market_state::MarketState,
    order_margin::compute_order_margin,
    prelude::Account,
    types::{Currency, LimitOrder, MarginCurrency, MarketOrder, Pending, QuoteCurrency, Side},
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

impl<M, UserOrderId> RiskEngine<M, UserOrderId> for IsolatedMarginRiskEngine<M>
where
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    fn check_market_order(
        &self,
        account: &Account<M, UserOrderId>,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
    ) -> Result<(), RiskError> {
        match order.side() {
            Side::Buy => self.handle_market_buy_order(account, order, fill_price),
            Side::Sell => self.handle_market_sell_order(account, order, fill_price),
        }
    }

    fn check_limit_order(
        &self,
        account: &Account<M, UserOrderId>,
        order: &LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
    ) -> Result<(), RiskError> {
        let mut orders = account.active_limit_orders.clone();
        orders.insert(order.state().meta().id(), order.clone());
        let new_order_margin = compute_order_margin(&account.position, &orders, account.leverage());

        // TODO: this calculation does not allow a fully loaded long (or short) position
        // to be reversed into the opposite position of the same size,
        // which should be possible and requires a slightly modified calculation that
        let available_balance = account.wallet_balance - account.position.position_margin;
        debug!(
            "new_order_margin: {}, available_balance: {}",
            new_order_margin, available_balance
        );
        let order_fees: M = fees_of_limit_orders(&orders, self.contract_spec.fee_maker);
        if new_order_margin + order_fees > available_balance {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn check_maintenance_margin(
        &self,
        market_state: &MarketState,
        account: &Account<M, UserOrderId>,
    ) -> Result<(), RiskError> {
        if account.position.size() == M::PairedCurrency::new_zero() {
            return Ok(());
        }
        let pos_value = account
            .position()
            .size()
            .abs()
            .convert(market_state.mid_price());
        let maint_margin = account
            .position()
            .size()
            .convert(account.position.entry_price)
            * self.contract_spec.maintenance_margin;
        if pos_value < maint_margin {
            return Err(RiskError::Liquidate);
        }

        Ok(())
    }
}

impl<M> IsolatedMarginRiskEngine<M>
where
    M: Currency + MarginCurrency,
    M::PairedCurrency: Currency,
{
    fn handle_market_buy_order<UserOrderId>(
        &self,
        account: &Account<M, UserOrderId>,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
    ) -> Result<(), RiskError>
    where
        UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    {
        debug_assert!(matches!(order.side(), Side::Buy));

        if account.position.size() >= M::PairedCurrency::new_zero() {
            // A long position increases in size.
            let notional_value = order.quantity().convert(fill_price);
            let margin_req = notional_value / account.leverage();
            let fee = notional_value * self.contract_spec.fee_taker;
            if margin_req + fee > account.available_balance() {
                return Err(RiskError::NotEnoughAvailableBalance);
            }
            return Ok(());
        }
        // Else its a short position which needs to be reduced
        if order.quantity() <= account.position.size().abs() {
            // The order strictly reduces the position, so no additional margin is required.
            return Ok(());
        }
        // The order reduces the short and puts on a long
        let released_from_old_pos = account.position.position_margin;

        let new_long_size = order.quantity() - account.position.size.abs();
        let new_notional_value = new_long_size.convert(fill_price);
        let new_margin_req = new_notional_value / account.leverage();

        if new_margin_req > account.available_balance() + released_from_old_pos {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn handle_market_sell_order<UserOrderId>(
        &self,
        account: &Account<M, UserOrderId>,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
    ) -> Result<(), RiskError>
    where
        UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    {
        debug_assert!(matches!(order.side(), Side::Sell));

        if account.position.size() <= M::PairedCurrency::new_zero() {
            let notional_value = order.quantity().convert(fill_price);
            let margin_req = notional_value / account.leverage();
            let fee = notional_value * self.contract_spec.fee_taker;
            if margin_req + fee > account.available_balance() {
                return Err(RiskError::NotEnoughAvailableBalance);
            }
            return Ok(());
        }
        // Else its a long position which needs to be reduced
        if order.quantity() <= account.position.size() {
            // The order strictly reduces the position, so no additional margin is required.
            return Ok(());
        }
        // The order reduces the long position and opens a short.
        let released_from_old_pos = account.position.position_margin;

        let new_short_size = order.quantity() - account.position.size();
        let new_margin_req = new_short_size.convert(fill_price) / account.leverage();

        if new_margin_req > account.available_balance() + released_from_old_pos {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }
}
