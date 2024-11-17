use tracing::trace;

use super::RiskEngine;
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{Currency, Mon, Position, QuoteCurrency, RiskError},
    types::{LimitOrder, MarginCurrency, MarketOrder, Pending, Side, UserOrderIdT},
};

#[derive(Debug, Clone)]
pub(crate) struct IsolatedMarginRiskEngine<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    contract_spec: ContractSpecification<I, D, BaseOrQuote>,
}

impl<I, const D: u8, BaseOrQuote> IsolatedMarginRiskEngine<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    pub(crate) fn new(contract_spec: ContractSpecification<I, D, BaseOrQuote>) -> Self {
        Self { contract_spec }
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> RiskEngine<I, D, BaseOrQuote, UserOrderId>
    for IsolatedMarginRiskEngine<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderId: UserOrderIdT,
{
    fn check_market_order(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        position_margin: BaseOrQuote::PairedCurrency,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
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
        position: &Position<I, D, BaseOrQuote>,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
        order_margin_online: &OrderMargin<I, D, BaseOrQuote, UserOrderId>,
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
        market_state: &MarketState<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> Result<(), RiskError> {
        let maint_margin_req = self.contract_spec.maintenance_margin();
        match position {
            Position::Neutral => return Ok(()),
            Position::Long(inner) => {
                let liquidation_price =
                    inner.entry_price().liquidation_price_long(maint_margin_req);
                if market_state.bid() < liquidation_price {
                    return Err(RiskError::Liquidate);
                }
            }
            Position::Short(inner) => {
                let liquidation_price = inner
                    .entry_price()
                    .liquidation_price_short(maint_margin_req);
                if market_state.ask() > liquidation_price {
                    return Err(RiskError::Liquidate);
                }
            }
        }

        Ok(())
    }
}

impl<I, const D: u8, BaseOrQuote> IsolatedMarginRiskEngine<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn check_market_buy_order<UserOrderId>(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        position_margin: BaseOrQuote::PairedCurrency,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
    ) -> Result<(), RiskError>
    where
        UserOrderId: UserOrderIdT,
    {
        assert!(matches!(order.side(), Side::Buy));

        match position {
            Position::Neutral | Position::Long(_) => {
                // A long position increases in size.
                let notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
                let margin_req = notional_value * self.contract_spec.init_margin_req();

                let fee = notional_value * *self.contract_spec.fee_taker().as_ref();
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

                let fee = new_notional_value * *self.contract_spec.fee_taker().as_ref();

                if new_margin_req + fee > available_wallet_balance + released_from_old_pos {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
        }

        Ok(())
    }

    fn check_market_sell_order<UserOrderId>(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        position_margin: BaseOrQuote::PairedCurrency,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
    ) -> Result<(), RiskError>
    where
        UserOrderId: UserOrderIdT,
    {
        assert!(matches!(order.side(), Side::Sell));

        match position {
            Position::Neutral | Position::Short(_) => {
                let notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
                let margin_req = notional_value * self.contract_spec.init_margin_req();
                let fee = notional_value * *self.contract_spec.fee_taker().as_ref();

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

                let fee = new_notional_value * *self.contract_spec.fee_taker().as_ref();

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
    use const_decimal::Decimal;
    use num_traits::One;

    use super::*;
    use crate::{prelude::*, test_fee_maker, test_fee_taker, MockTransactionAccounting, DECIMALS};

    #[test_case::test_case(2, 75)]
    #[test_case::test_case(3, 84)]
    #[test_case::test_case(5, 90)]
    fn isolated_margin_check_maintenance_margin_long(leverage: u8, expected_liq_price: i64) {
        let contract_spec = ContractSpecification::<_, DECIMALS, BaseCurrency<_, DECIMALS>>::new(
            Leverage::new(leverage).unwrap(),
            Decimal::try_from_scaled(5, 1).unwrap(),
            PriceFilter::default(),
            QuantityFilter::default(),
            test_fee_maker(),
            test_fee_taker(),
        )
        .unwrap();
        let init_margin_req = contract_spec.init_margin_req();
        let re =
            IsolatedMarginRiskEngine::<_, DECIMALS, BaseCurrency<_, DECIMALS>>::new(contract_spec);
        let market_state = MarketState::from_components(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            0.into(),
            0,
        );
        let mut accounting = MockTransactionAccounting::default();

        let position = Position::Neutral;

        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let qty = BaseCurrency::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let fees = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_maker().as_ref();
        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
        ));
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let position = Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
        ));
        let market_state = MarketState::from_components(
            QuoteCurrency::new(200, 0),
            QuoteCurrency::new(201, 0),
            0.into(),
            0,
        );
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let ask = QuoteCurrency::new(expected_liq_price, 0);
        let bid = ask - QuoteCurrency::one();
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        assert_eq!(
            RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
                &re,
                &market_state,
                &position
            ),
            Err(RiskError::Liquidate)
        );
        let ask = QuoteCurrency::new(expected_liq_price, 0) + QuoteCurrency::one();
        let bid = ask - QuoteCurrency::one();
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();
    }

    #[test_case::test_case(2, 126)]
    #[test_case::test_case(3, 117)]
    #[test_case::test_case(5, 111)]
    fn isolated_margin_check_maintenance_margin_short(leverage: u8, expected_liq_price: i64) {
        let contract_spec = ContractSpecification::<_, DECIMALS, BaseCurrency<_, DECIMALS>>::new(
            Leverage::new(leverage).unwrap(),
            Decimal::try_from_scaled(5, 1).unwrap(),
            PriceFilter::default(),
            QuantityFilter::default(),
            test_fee_maker(),
            test_fee_taker(),
        )
        .unwrap();
        let init_margin_req = contract_spec.init_margin_req();
        let re =
            IsolatedMarginRiskEngine::<_, DECIMALS, BaseCurrency<_, DECIMALS>>::new(contract_spec);
        let market_state = MarketState::from_components(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            0.into(),
            0,
        );
        let mut accounting = MockTransactionAccounting::default();

        let qty = BaseCurrency::one();
        let entry_price = QuoteCurrency::new(100, 0);
        let fees = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_maker().as_ref();
        let position = Position::Short(PositionInner::new(
            BaseCurrency::one(),
            QuoteCurrency::new(100, 0),
            &mut accounting,
            init_margin_req,
            fees,
        ));
        RiskEngine::<i64, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let ask = QuoteCurrency::new(expected_liq_price, 0);
        let bid = ask - QuoteCurrency::one();
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        assert_eq!(
            RiskEngine::<i64, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
                &re,
                &market_state,
                &position
            ),
            Err(RiskError::Liquidate)
        );
        let ask = QuoteCurrency::new(expected_liq_price, 0) - QuoteCurrency::one();
        let bid = ask - QuoteCurrency::one();
        let market_state = MarketState::from_components(bid, ask, 0.into(), 0);
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();
    }
}
