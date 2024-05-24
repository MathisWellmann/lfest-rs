use super::{risk_engine_trait::RiskError, RiskEngine};
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    order_margin::OrderMarginOnline,
    prelude::Position,
    types::{Currency, LimitOrder, MarginCurrency, MarketOrder, Pending, QuoteCurrency, Side},
};

/// TODO: change M to Q
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
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash + Default,
{
    fn check_market_order(
        &self,
        position: &Position<M::PairedCurrency>,
        position_margin: M,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
        available_wallet_balance: M,
    ) -> Result<(), RiskError> {
        match order.side() {
            Side::Buy => self.check_market_buy_order(
                position,
                position_margin,
                order,
                fill_price,
                available_wallet_balance,
            ),
            Side::Sell => self.check_market_sell_order(
                position,
                position_margin,
                order,
                fill_price,
                available_wallet_balance,
            ),
        }
    }

    fn check_limit_order(
        &self,
        position: &Position<M::PairedCurrency>,
        position_margin: M,
        order: &LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        available_wallet_balance: M,
        order_margin_online: &OrderMarginOnline<M::PairedCurrency, UserOrderId>,
    ) -> Result<(), RiskError> {
        let order_margin = order_margin_online.order_margin(
            self.contract_spec.init_margin_req(),
            position,
            position_margin,
        );
        let new_order_margin = order_margin_online.order_margin_with_order(
            order,
            self.contract_spec.init_margin_req(),
            position,
            position_margin,
        );

        let order_fees: M = order_margin_online.cumulative_order_fees();

        if new_order_margin + order_fees > available_wallet_balance + order_margin {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn check_maintenance_margin(
        &self,
        market_state: &MarketState,
        position: &Position<M::PairedCurrency>,
    ) -> Result<(), RiskError> {
        let pos_inner = match position {
            Position::Neutral => return Ok(()),
            Position::Long(inner) => inner,
            Position::Short(inner) => inner,
        };
        if pos_inner.quantity() == M::PairedCurrency::new_zero() {
            return Ok(());
        }
        let pos_value = pos_inner.quantity().abs().convert(market_state.mid_price());
        let maint_margin = pos_inner.quantity().convert(pos_inner.entry_price())
            * self.contract_spec.maintenance_margin();
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
    fn check_market_buy_order<UserOrderId>(
        &self,
        position: &Position<M::PairedCurrency>,
        position_margin: M,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
        available_wallet_balance: M,
    ) -> Result<(), RiskError>
    where
        UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    {
        assert!(matches!(order.side(), Side::Buy));

        match position {
            Position::Neutral | Position::Long(_) => {}
            Position::Short(pos_inner) => {
                if order.quantity() <= pos_inner.quantity() {
                    // The order strictly reduces the position, so no additional margin is required.
                    return Ok(());
                }
                // The order reduces the short and puts on a long
                let released_from_old_pos = position_margin;

                let new_long_size = order.quantity() - pos_inner.quantity();
                let new_notional_value = new_long_size.convert(fill_price);
                let new_margin_req = new_notional_value * self.contract_spec.init_margin_req();

                let fee = new_notional_value * self.contract_spec.fee_taker();

                if new_margin_req + fee > available_wallet_balance + released_from_old_pos {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
        }
        // A long position increases in size.
        let notional_value = order.quantity().convert(fill_price);
        let margin_req = notional_value * self.contract_spec.init_margin_req();

        let fee = notional_value * self.contract_spec.fee_taker();
        if margin_req + fee > available_wallet_balance {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn check_market_sell_order<UserOrderId>(
        &self,
        position: &Position<M::PairedCurrency>,
        position_margin: M,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
        available_wallet_balance: M,
    ) -> Result<(), RiskError>
    where
        UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    {
        assert!(matches!(order.side(), Side::Sell));

        match position {
            Position::Neutral | Position::Short(_) => {}
            Position::Long(pos_inner) => {
                // Else its a long position which needs to be reduced
                if order.quantity() <= pos_inner.quantity() {
                    // The order strictly reduces the position, so no additional margin is required.
                    return Ok(());
                }
                // The order reduces the long position and opens a short.
                let released_from_old_pos = position_margin;

                let new_short_size = order.quantity() - pos_inner.quantity();
                let new_notional_value = new_short_size.convert(fill_price);
                let new_margin_req = new_notional_value * self.contract_spec.init_margin_req();

                let fee = new_notional_value * self.contract_spec.fee_taker();

                if new_margin_req + fee > available_wallet_balance + released_from_old_pos {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
        }
        let notional_value = order.quantity().convert(fill_price);
        let margin_req = notional_value * self.contract_spec.init_margin_req();
        let fee = notional_value * self.contract_spec.fee_taker();

        if margin_req + fee > available_wallet_balance {
            return Err(RiskError::NotEnoughAvailableBalance);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn isolated_margin_check_market_buy_order() {
        todo!()
    }

    #[test]
    fn isolated_margin_check_market_sell_order() {
        todo!()
    }
}
