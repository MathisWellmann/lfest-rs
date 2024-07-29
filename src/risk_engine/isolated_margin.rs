use fpdec::{Dec, Decimal};
use tracing::trace;

use super::RiskEngine;
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{Position, RiskError},
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
        order: &LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        available_wallet_balance: M,
        order_margin_online: &OrderMargin<M::PairedCurrency, UserOrderId>,
    ) -> Result<(), RiskError> {
        let order_margin = order_margin_online
            .order_margin_with_fees(self.contract_spec.init_margin_req(), position);
        let new_order_margin = order_margin_online.order_margin_and_fees_with_order(
            order,
            self.contract_spec.init_margin_req(),
            position,
        );

        trace!("order_margin: {order_margin}, new_order_margin: {new_order_margin}, available_wallet_balance: {available_wallet_balance}");
        if new_order_margin > available_wallet_balance + order_margin {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn check_maintenance_margin(
        &self,
        market_state: &MarketState,
        position: &Position<M::PairedCurrency>,
    ) -> Result<(), RiskError> {
        let maint_margin_req = self.contract_spec.maintenance_margin();
        match position {
            Position::Neutral => return Ok(()),
            Position::Long(inner) => {
                let liquidation_price = inner.entry_price().as_ref() * (Dec!(1) - maint_margin_req);
                if market_state.bid().as_ref() < &liquidation_price {
                    return Err(RiskError::Liquidate);
                }
            }
            Position::Short(inner) => {
                let liquidation_price = inner.entry_price().as_ref() * (Dec!(1) + maint_margin_req);
                if market_state.ask().as_ref() > &liquidation_price {
                    return Err(RiskError::Liquidate);
                }
            }
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
            Position::Neutral | Position::Long(_) => {
                // A long position increases in size.
                let notional_value = order.quantity().convert(fill_price);
                let margin_req = notional_value * self.contract_spec.init_margin_req();

                let fee = notional_value * self.contract_spec.fee_taker();
                if margin_req + fee > available_wallet_balance {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
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
            Position::Neutral | Position::Short(_) => {
                let notional_value = order.quantity().convert(fill_price);
                let margin_req = notional_value * self.contract_spec.init_margin_req();
                let fee = notional_value * self.contract_spec.fee_taker();

                if margin_req + fee > available_wallet_balance {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fpdec::{Dec, Decimal};

    use super::*;
    use crate::{
        base, fee,
        prelude::{BaseCurrency, Leverage, PositionInner, PriceFilter, QuantityFilter},
        quote, MockTransactionAccounting,
    };

    #[test_case::test_case(2, 75)]
    #[test_case::test_case(3, 84)]
    #[test_case::test_case(5, 90)]
    fn isolated_margin_check_maintenance_margin_long(leverage: u32, expected_liq_price: u32) {
        let contract_spec = ContractSpecification::<BaseCurrency>::new(
            Leverage::new(Decimal::from(leverage)).unwrap(),
            Dec!(0.5),
            PriceFilter::default(),
            QuantityFilter::default(),
            fee!(0.0002),
            fee!(0.0006),
        )
        .unwrap();
        let init_margin_req = contract_spec.init_margin_req();
        let re = IsolatedMarginRiskEngine::<QuoteCurrency>::new(contract_spec);
        let market_state = MarketState::from_components(quote!(100), quote!(101), 0.into(), 0);
        let mut accounting = MockTransactionAccounting::default();

        let position = Position::Neutral;

        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();

        let position = Position::Long(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();

        let position = Position::Long(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        let market_state = MarketState::from_components(quote!(200), quote!(201), 0.into(), 0);
        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();

        let ask = QuoteCurrency::new(Decimal::from(expected_liq_price));
        let bid = ask - quote!(1);
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        assert_eq!(
            RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position),
            Err(RiskError::Liquidate)
        );
        let ask = QuoteCurrency::new(Decimal::from(expected_liq_price)) + quote!(1);
        let bid = ask - quote!(1);
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();
    }

    #[test_case::test_case(2, 126)]
    #[test_case::test_case(3, 117)]
    #[test_case::test_case(5, 111)]
    fn isolated_margin_check_maintenance_margin_short(leverage: u32, expected_liq_price: u32) {
        let contract_spec = ContractSpecification::<BaseCurrency>::new(
            Leverage::new(Decimal::from(leverage)).unwrap(),
            Dec!(0.5),
            PriceFilter::default(),
            QuantityFilter::default(),
            fee!(0.0002),
            fee!(0.0006),
        )
        .unwrap();
        let init_margin_req = contract_spec.init_margin_req();
        let re = IsolatedMarginRiskEngine::<QuoteCurrency>::new(contract_spec);
        let market_state = MarketState::from_components(quote!(100), quote!(101), 0.into(), 0);
        let mut accounting = MockTransactionAccounting::default();

        let position = Position::Short(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ));
        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();

        let ask = QuoteCurrency::new(Decimal::from(expected_liq_price));
        let bid = ask - quote!(1);
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        assert_eq!(
            RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position),
            Err(RiskError::Liquidate)
        );
        let ask = QuoteCurrency::new(Decimal::from(expected_liq_price)) - quote!(1);
        let bid = ask - quote!(1);
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();
    }
}
