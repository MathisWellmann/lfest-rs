use const_decimal::Decimal;
use num::Zero;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};
use tracing::trace;

use super::RiskEngine;
use crate::{
    account::{
        Account,
        Position,
        PositionSide,
    },
    contract_specification::ContractSpecification,
    market_state::MarketState,
    prelude::{
        Currency,
        Mon,
        QuoteCurrency,
        RiskError,
    },
    types::{
        LimitOrder,
        MarginCurrency,
        MarketOrder,
        NotEnoughAvailableBalance,
        Pending,
        Side,
        UserOrderId,
    },
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
pub(crate) struct IsolatedMarginRiskEngine<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    contract_spec: ContractSpecification<I, D, BaseOrQuote>,
}

/// Static entry-price liquidation threshold of a non-neutral `position` in isolated mode.
///
/// The maintenance rate comes from the contract's tiered schedule, evaluated at the
/// position's entry notional, so large positions use their bracket's (higher) rate
/// instead of the tier-0 rate.
#[must_use]
pub(crate) fn isolated_liquidation_price<I, const D: u8, BaseOrQuote>(
    contract_spec: &ContractSpecification<I, D, BaseOrQuote>,
    position: &Position<I, D, BaseOrQuote>,
) -> QuoteCurrency<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    assert!(!position.quantity().is_zero());
    let notional = position.notional();
    assert!(notional > Zero::zero());
    let mm_rate = contract_spec
        .maintenance_margin_schedule()
        .effective_rate(notional);
    let init_rate = contract_spec.init_margin_req();
    // `<=`: `ContractSpecification::new` accepts a maintenance fraction of 1.
    assert!(mm_rate <= init_rate, "validated by the contract spec");
    // Isolated margin posts `imr` of the notional and liquidates once the loss eats into
    // the maintenance part, i.e. after a relative move of `imr - mmr`. A higher tier rate
    // must therefore move the liquidation price CLOSER to the entry. The former
    // `entry * (1 -/+ mmr)` only coincided with this for `mmr == imr / 2` (the default
    // maintenance fraction) and moved it the wrong way for higher tiers.
    let max_move = init_rate - mm_rate;
    assert!(max_move >= Decimal::zero());
    if position.side() == PositionSide::Long {
        position.entry_price().liquidation_price_long(max_move)
    } else {
        position.entry_price().liquidation_price_short(max_move)
    }
}

impl<I, const D: u8, BaseOrQuote> IsolatedMarginRiskEngine<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
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
        account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
    ) -> Result<(), NotEnoughAvailableBalance> {
        let position_qty = account.position().quantity();
        let reduces_position = match order.side() {
            Side::Buy => position_qty.is_negative(),
            Side::Sell => position_qty.is_positive(),
        };
        let (opened_qty, released_margin) = if reduces_position {
            let abs_qty = position_qty.abs();
            if order.quantity() <= abs_qty {
                // The order strictly reduces the position, so no additional margin is required
                // and a risk-reducing order is never rejected. Any collateral shortfall its
                // settlement causes (fees, realized losses, resting orders losing their
                // position offset) is resolved by `Exchange::reconcile_margin`, which
                // force-cancels resting limit orders like a real venue's margin call.
                return Ok(());
            }
            // The order closes the position, releasing its margin, and opens the rest on
            // the other side.
            (order.quantity() - abs_qty, account.position_margin())
        } else {
            (order.quantity(), Zero::zero())
        };
        assert2::debug_assert!(opened_qty > BaseOrQuote::zero());
        assert2::debug_assert!(opened_qty <= order.quantity());

        let notional = BaseOrQuote::PairedCurrency::convert_from(opened_qty, fill_price);
        let init_margin = notional * self.contract_spec.init_margin_req();
        let fee = notional * *self.contract_spec.fee_taker().as_ref();
        if init_margin + fee > account.available_balance() + released_margin {
            return Err(NotEnoughAvailableBalance);
        }
        Ok(())
    }

    fn check_limit_order(
        &self,
        account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), NotEnoughAvailableBalance> {
        // Admission is decided by the canonical collateral requirement of the account:
        // with the new order resting, the position margin, the order margin and the
        // reserved maker fees must still be covered by the account equity.
        let excess = account.margin_excess_with_order(order);
        trace!("check_limit_order: margin_excess_with_order: {excess:?}");
        if excess < Zero::zero() {
            return Err(NotEnoughAvailableBalance);
        }

        Ok(())
    }

    fn check_maintenance_margin(
        &self,
        market_state: &MarketState<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> Result<(), RiskError> {
        let side = position.side();
        if side == PositionSide::Neutral {
            return Ok(());
        }
        let liquidation_price = isolated_liquidation_price(&self.contract_spec, position);
        let reference_price =
            market_state.mark_price_for_position(self.contract_spec.mark_method(), side);
        // A long is liquidated below its liquidation price, a short above it.
        let breached = if side == PositionSide::Long {
            reference_price < liquidation_price
        } else {
            reference_price > liquidation_price
        };
        if breached {
            return Err(RiskError::Liquidate);
        }
        Ok(())
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

    type Base = BaseCurrency<i64, DECIMALS>;
    type Acc = Account<i64, DECIMALS, Base, NoUserOrderId>;

    /// Fee-free engine with 2x leverage and an account of 1000 quote holding
    /// `position_qty` (signed) entered at 100.
    fn engine_and_account(
        position_qty: i64,
    ) -> (IsolatedMarginRiskEngine<i64, DECIMALS, Base>, Acc) {
        let spec = ContractSpecification::new(
            Leverage::new(2).unwrap(),
            Decimal::try_from_scaled(5, 1).unwrap(),
            PriceFilter::default(),
            QuantityFilter::default(),
            Fee::from(Decimal::zero()),
            Fee::from(Decimal::zero()),
        )
        .unwrap();
        let mut account = Acc::new(
            Balances::new(QuoteCurrency::new(1000, 0)),
            std::num::NonZeroU16::new(10).unwrap(),
            spec.init_margin_req(),
            Decimal::zero(),
        );
        if position_qty != 0 {
            let side = if position_qty > 0 {
                Side::Buy
            } else {
                Side::Sell
            };
            let realized = account.change_position(
                Base::new(position_qty.abs(), 0),
                QuoteCurrency::new(100, 0),
                side,
                QuoteCurrency::zero(),
            );
            assert_eq!(realized, QuoteCurrency::zero());
        }
        (IsolatedMarginRiskEngine::new(spec), account)
    }

    /// Check a market order of `qty_scaled` (5 decimals) filled at 100.
    fn check_market(
        position_qty: i64,
        side: Side,
        qty_scaled: i64,
    ) -> Result<(), NotEnoughAvailableBalance> {
        let (engine, account) = engine_and_account(position_qty);
        let order = MarketOrder::new(side, Base::new(qty_scaled, 5))
            .unwrap()
            .into_pending(ExchangeOrderMeta::new(0.into(), 0.into()));
        engine.check_market_order(&account, &order, QuoteCurrency::new(100, 0))
    }

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn isolated_market_order_may_use_exactly_the_available_balance(side: Side) {
        // 20 at 100 with 2x leverage needs all of the 1000.
        assert_eq!(check_market(0, side, 20_00000), Ok(()));
        assert_eq!(
            check_market(0, side, 20_00001),
            Err(NotEnoughAvailableBalance)
        );
    }

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn isolated_market_order_increasing_a_position_uses_the_available_balance(side: Side) {
        let position_qty = if side == Side::Buy { 2 } else { -2 };
        // The position of 2 locks 100, leaving 900 for 18 more.
        assert_eq!(check_market(position_qty, side, 18_00000), Ok(()));
        assert_eq!(
            check_market(position_qty, side, 18_00001),
            Err(NotEnoughAvailableBalance)
        );
    }

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn isolated_market_order_reducing_a_position_needs_no_margin(side: Side) {
        let position_qty = if side == Side::Buy { -2 } else { 2 };
        assert_eq!(check_market(position_qty, side, 1_00000), Ok(()));
        assert_eq!(check_market(position_qty, side, 2_00000), Ok(()));
    }

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn isolated_market_order_flipping_a_position_reuses_its_margin(side: Side) {
        let position_qty = if side == Side::Buy { -2 } else { 2 };
        // Closing the 2 releases their 100 to the 900 available: room for 20 on the other side.
        assert_eq!(check_market(position_qty, side, 22_00000), Ok(()));
        assert_eq!(
            check_market(position_qty, side, 22_00001),
            Err(NotEnoughAvailableBalance)
        );
    }

    #[test]
    fn isolated_limit_order_may_use_exactly_the_available_balance() {
        let (engine, account) = engine_and_account(0);
        let check = |qty_scaled| {
            let order = LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                Base::new(qty_scaled, 5),
            )
            .unwrap()
            .into_pending(ExchangeOrderMeta::new(0.into(), 0.into()));
            RiskEngine::check_limit_order(&engine, &account, &order)
        };
        assert_eq!(check(20_00000), Ok(()));
        assert_eq!(check(20_00001), Err(NotEnoughAvailableBalance));
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
        let re =
            IsolatedMarginRiskEngine::<_, DECIMALS, BaseCurrency<_, DECIMALS>>::new(contract_spec);
        let market_state = MarketState::from_components(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            QuoteCurrency::new(101, 0),
            0.into(),
            0,
        );

        let position = Position::default();

        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let qty = BaseCurrency::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Position::new(qty, entry_price).unwrap();
        RiskEngine::<_, DECIMALS, _, NoUserOrderId>::check_maintenance_margin(
            &re,
            &market_state,
            &position,
        )
        .unwrap();

        let position = Position::new(qty, entry_price).unwrap();
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

        let position = Position::new(-BaseCurrency::one(), QuoteCurrency::new(100, 0)).unwrap();
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
