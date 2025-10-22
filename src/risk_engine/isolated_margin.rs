use num::Zero;
use tracing::trace;

use super::RiskEngine;
use crate::{
    contract_specification::ContractSpecification,
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{
        Currency,
        Mon,
        Position,
        PositionInner,
        QuoteCurrency,
        RiskError,
    },
    types::{
        Balances,
        LimitOrder,
        MarginCurrency,
        MarketOrder,
        Pending,
        Side,
        UserOrderId,
    },
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

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> RiskEngine<I, D, BaseOrQuote, UserOrderIdT>
    for IsolatedMarginRiskEngine<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    fn check_market_order(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        balances: &Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<(), RiskError> {
        match order.side() {
            Side::Buy => self.check_market_buy_order(position, order, fill_price, balances),
            Side::Sell => self.check_market_sell_order(position, order, fill_price, balances),
        }
    }

    fn check_limit_order(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        available_balance: BaseOrQuote::PairedCurrency,
        order_margin: &OrderMargin<I, D, BaseOrQuote, UserOrderIdT>,
    ) -> Result<(), RiskError> {
        let om = order_margin.order_margin(self.contract_spec.init_margin_req(), position);
        let new_order_margin = order_margin.order_margin_with_order(
            order,
            self.contract_spec.init_margin_req(),
            position,
        );

        trace!(
            "order_margin: {om:?}, new_order_margin: {new_order_margin:?}, available_balance: {available_balance:?}"
        );
        if new_order_margin > available_balance + om {
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
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    fn check_market_buy_order<UserOrderIdT>(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        balances: &Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<(), RiskError>
    where
        UserOrderIdT: UserOrderId,
    {
        debug_assert_eq!(order.side(), Side::Buy);

        match position {
            Position::Neutral | Position::Long(_) => {
                // A long position increases in size.
                let notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
                let init_margin = notional_value * self.contract_spec.init_margin_req();

                let fee = notional_value * *self.contract_spec.fee_taker().as_ref();
                if init_margin + fee > balances.available() {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
            Position::Short(pos_inner) => {
                if order.quantity() <= pos_inner.quantity() {
                    // The order strictly reduces the position, so no additional margin is required.
                    return Ok(());
                }
                // The order reduces the short and puts on a long
                let released_from_old_pos = balances.position_margin();

                let new_long_size = Self::quantity_minus_position(order.quantity(), pos_inner);
                assert2::debug_assert!(new_long_size > BaseOrQuote::zero());
                let new_notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(new_long_size, fill_price);
                assert2::debug_assert!(new_notional_value > BaseOrQuote::PairedCurrency::zero());
                let new_init_margin = new_notional_value * self.contract_spec.init_margin_req();
                assert2::debug_assert!(new_init_margin > BaseOrQuote::PairedCurrency::zero());

                let fee = new_notional_value * *self.contract_spec.fee_taker().as_ref();

                if Self::margin_exceeds_risk(
                    new_init_margin,
                    fee,
                    balances.available(),
                    released_from_old_pos,
                ) {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
        }

        Ok(())
    }

    fn check_market_sell_order<UserOrderIdT>(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        balances: &Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<(), RiskError>
    where
        UserOrderIdT: UserOrderId,
    {
        debug_assert_eq!(order.side(), Side::Sell);

        match position {
            Position::Neutral | Position::Short(_) => {
                let notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
                let init_margin = notional_value * self.contract_spec.init_margin_req();
                let fee = notional_value * *self.contract_spec.fee_taker().as_ref();

                if init_margin + fee > balances.available() {
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
                let released_from_old_pos = balances.position_margin();

                let new_short_size = Self::quantity_minus_position(order.quantity(), pos_inner);
                assert2::debug_assert!(new_short_size > BaseOrQuote::zero());
                let new_notional_value =
                    BaseOrQuote::PairedCurrency::convert_from(new_short_size, fill_price);
                assert2::debug_assert!(new_notional_value > BaseOrQuote::PairedCurrency::zero());
                let new_init_margin = new_notional_value * self.contract_spec.init_margin_req();
                assert2::debug_assert!(new_init_margin > BaseOrQuote::PairedCurrency::zero());

                let fee = new_notional_value * *self.contract_spec.fee_taker().as_ref();

                if Self::margin_exceeds_risk(
                    new_init_margin,
                    fee,
                    balances.available(),
                    released_from_old_pos,
                ) {
                    return Err(RiskError::NotEnoughAvailableBalance);
                }
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn margin_exceeds_risk(
        new_margin_req: BaseOrQuote::PairedCurrency,
        tx_fee: BaseOrQuote::PairedCurrency,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
        released_margin_from_old_pos: BaseOrQuote::PairedCurrency,
    ) -> bool {
        new_margin_req + tx_fee > available_wallet_balance + released_margin_from_old_pos
    }

    #[inline(always)]
    fn quantity_minus_position(
        quantity: BaseOrQuote,
        position_inner: &PositionInner<I, D, BaseOrQuote>,
    ) -> BaseOrQuote {
        quantity - position_inner.quantity()
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;
    use num_traits::One;

    use super::*;
    use crate::{
        DECIMALS,
        prelude::*,
        test_fee_maker,
        test_fee_taker,
    };

    #[test]
    fn isolated_margin_exceeds_risk() {
        assert!(
            !IsolatedMarginRiskEngine::<i64, 5, BaseCurrency<i64, 5>>::margin_exceeds_risk(
                QuoteCurrency::<i64, 5>::new(10, 0),
                QuoteCurrency::new(1, 1),
                QuoteCurrency::new(1000, 0),
                QuoteCurrency::new(0, 0)
            )
        );
        assert!(
            IsolatedMarginRiskEngine::<i64, 5, BaseCurrency<i64, 5>>::margin_exceeds_risk(
                QuoteCurrency::<i64, 5>::new(1000, 0),
                QuoteCurrency::new(1, 1),
                QuoteCurrency::new(1000, 0),
                QuoteCurrency::new(0, 0)
            )
        );
        assert!(
            !IsolatedMarginRiskEngine::<i64, 5, BaseCurrency<i64, 5>>::margin_exceeds_risk(
                QuoteCurrency::<i64, 5>::new(1000, 0),
                QuoteCurrency::new(1, 1),
                QuoteCurrency::new(1000, 0),
                QuoteCurrency::new(1, 0)
            )
        );
    }

    #[test]
    fn isolated_margin_quantity_minus_position() {
        assert_eq!(
            IsolatedMarginRiskEngine::quantity_minus_position(
                BaseCurrency::new(10, 0),
                &PositionInner::from_parts(
                    BaseCurrency::<i64, 5>::new(5, 0),
                    QuoteCurrency::new(100, 0),
                )
            ),
            BaseCurrency::new(5, 0)
        );
    }

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
            QuoteCurrency::new(101, 0),
            0.into(),
            0,
        );

        let position = Position::Neutral;

        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let qty = BaseCurrency::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);

        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Position::Long(PositionInner::new(qty, entry_price));
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let position = Position::Long(PositionInner::new(qty, entry_price));
        let market_state = MarketState::from_components(
            QuoteCurrency::new(200, 0),
            QuoteCurrency::new(201, 0),
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
        let market_state = MarketState::from_components(bid, ask, ask, 0.into(), 0);
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
        let market_state = MarketState::from_components(bid, ask, ask, 0.into(), 0);
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
        let re =
            IsolatedMarginRiskEngine::<_, DECIMALS, BaseCurrency<_, DECIMALS>>::new(contract_spec);
        let market_state = MarketState::from_components(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            QuoteCurrency::new(101, 0),
            0.into(),
            0,
        );

        let position = Position::Short(PositionInner::new(
            BaseCurrency::one(),
            QuoteCurrency::new(100, 0),
        ));
        RiskEngine::<i64, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let ask = QuoteCurrency::new(expected_liq_price, 0);
        let bid = ask - QuoteCurrency::one();
        let market_state = MarketState::from_components(bid, ask, ask, 0.into(), 0);
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
        let market_state = MarketState::from_components(bid, ask, ask, 0.into(), 0);
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();
    }
}
