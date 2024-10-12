use tracing::trace;

use super::RiskEngine;
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{CurrencyMarker, Mon, Monies, Position, Quote, RiskError},
    types::{LimitOrder, MarginCurrencyMarker, MarketOrder, Pending, Side},
};

#[derive(Debug, Clone)]
pub(crate) struct IsolatedMarginRiskEngine<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    contract_spec: ContractSpecification<T, BaseOrQuote>,
}

impl<T, BaseOrQuote> IsolatedMarginRiskEngine<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    pub(crate) fn new(contract_spec: ContractSpecification<T, BaseOrQuote>) -> Self {
        Self { contract_spec }
    }
}

impl<T, BaseOrQuote, UserOrderId> RiskEngine<T, BaseOrQuote, UserOrderId>
    for IsolatedMarginRiskEngine<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash + Default,
{
    fn check_market_order(
        &self,
        position: &Position<T, BaseOrQuote>,
        position_margin: Monies<T, BaseOrQuote::PairedCurrency>,
        order: &MarketOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
        fill_price: Monies<T, Quote>,
        available_wallet_balance: Monies<T, BaseOrQuote::PairedCurrency>,
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
        position: &Position<T, BaseOrQuote>,
        order: &LimitOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
        available_wallet_balance: Monies<T, BaseOrQuote::PairedCurrency>,
        order_margin_online: &OrderMargin<T, BaseOrQuote, UserOrderId>,
    ) -> Result<(), RiskError> {
        let order_margin =
            order_margin_online.order_margin(self.contract_spec.init_margin_req(), position);
        let new_order_margin = order_margin_online.order_margin_with_order(
            order,
            self.contract_spec.init_margin_req(),
            position,
        );

        trace!("order_margin: {order_margin:?}, new_order_margin: {new_order_margin:?}, available_wallet_balance: {available_wallet_balance:?}");
        if new_order_margin > available_wallet_balance + order_margin {
            return Err(RiskError::NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn check_maintenance_margin(
        &self,
        market_state: &MarketState<T>,
        position: &Position<T, BaseOrQuote>,
    ) -> Result<(), RiskError> {
        let maint_margin_req = self.contract_spec.maintenance_margin();
        match position {
            Position::Neutral => return Ok(()),
            Position::Long(inner) => {
                let liquidation_price = inner.entry_price().liquidation_price(maint_margin_req);
                if market_state.bid() < liquidation_price {
                    return Err(RiskError::Liquidate);
                }
            }
            Position::Short(inner) => {
                let liquidation_price = inner.entry_price().liquidation_price(maint_margin_req);
                if market_state.ask() > liquidation_price {
                    return Err(RiskError::Liquidate);
                }
            }
        }

        Ok(())
    }
}

impl<T, BaseOrQuote> IsolatedMarginRiskEngine<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    // BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
{
    fn check_market_buy_order<UserOrderId>(
        &self,
        position: &Position<T, BaseOrQuote>,
        position_margin: Monies<T, BaseOrQuote::PairedCurrency>,
        order: &MarketOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
        fill_price: Monies<T, Quote>,
        available_wallet_balance: Monies<T, BaseOrQuote::PairedCurrency>,
    ) -> Result<(), RiskError>
    where
        UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    {
        assert!(matches!(order.side(), Side::Buy));

        match position {
            Position::Neutral | Position::Long(_) => {
                // A long position increases in size.
                let notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
                let margin_req = notional_value * self.contract_spec.init_margin_req();

                let fee = self.contract_spec.fee_taker().for_value(notional_value);
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
                let new_notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(new_long_size, fill_price);
                let new_margin_req = new_notional_value * self.contract_spec.init_margin_req();

                let fee = self.contract_spec.fee_taker().for_value(new_notional_value);

                if new_margin_req + fee > available_wallet_balance + released_from_old_pos {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
        }

        Ok(())
    }

    fn check_market_sell_order<UserOrderId>(
        &self,
        position: &Position<T, BaseOrQuote>,
        position_margin: Monies<T, BaseOrQuote::PairedCurrency>,
        order: &MarketOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
        fill_price: Monies<T, Quote>,
        available_wallet_balance: Monies<T, BaseOrQuote::PairedCurrency>,
    ) -> Result<(), RiskError>
    where
        UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    {
        assert!(matches!(order.side(), Side::Sell));

        match position {
            Position::Neutral | Position::Short(_) => {
                let notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
                let margin_req = notional_value * self.contract_spec.init_margin_req();
                let fee = self.contract_spec.fee_taker().for_value(notional_value);

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
                let new_notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(new_short_size, fill_price);
                let new_margin_req = new_notional_value * self.contract_spec.init_margin_req();

                let fee = self.contract_spec.fee_taker().for_value(new_notional_value);

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
        prelude::{Leverage, PositionInner, PriceFilter, QuantityFilter},
        MockTransactionAccounting, TEST_FEE_MAKER, TEST_FEE_TAKER,
    };

    #[test_case::test_case(2, 75)]
    #[test_case::test_case(3, 84)]
    #[test_case::test_case(5, 90)]
    fn isolated_margin_check_maintenance_margin_long(leverage: u8, expected_liq_price: u32) {
        let contract_spec = ContractSpecification::<BaseCurrency>::new(
            Leverage::new(leverage).unwrap(),
            Dec!(0.5),
            PriceFilter::default(),
            QuantityFilter::default(),
            TEST_FEE_MAKER,
            TEST_FEE_TAKER,
        )
        .unwrap();
        let init_margin_req = contract_spec.init_margin_req();
        let re = IsolatedMarginRiskEngine::<QuoteCurrency>::new(contract_spec);
        let market_state = MarketState::from_components(quote!(100), quote!(101), 0.into(), 0);
        let mut accounting = MockTransactionAccounting::default();

        let position = Position::Neutral;

        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();

        let qty = base!(1);
        let entry_price = quote!(100);
        let fees = qty.convert(entry_price) * TEST_FEE_MAKER;
        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
        ));
        RiskEngine::<_, ()>::check_maintenance_margin(&re, &market_state, &position).unwrap();

        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
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
    fn isolated_margin_check_maintenance_margin_short(leverage: u8, expected_liq_price: u32) {
        let contract_spec = ContractSpecification::<BaseCurrency>::new(
            Leverage::new(leverage).unwrap(),
            Dec!(0.5),
            PriceFilter::default(),
            QuantityFilter::default(),
            TEST_FEE_MAKER,
            TEST_FEE_TAKER,
        )
        .unwrap();
        let init_margin_req = contract_spec.init_margin_req();
        let re = IsolatedMarginRiskEngine::<QuoteCurrency>::new(contract_spec);
        let market_state = MarketState::from_components(quote!(100), quote!(101), 0.into(), 0);
        let mut accounting = MockTransactionAccounting::default();

        let qty = base!(1);
        let entry_price = quote!(100);
        let fees = qty.convert(entry_price) * TEST_FEE_MAKER;
        let position = Position::Short(PositionInner::new(
            base!(1),
            quote!(100),
            &mut accounting,
            init_margin_req,
            fees,
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
