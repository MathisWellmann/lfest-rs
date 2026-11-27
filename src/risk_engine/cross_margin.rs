//! Cross-margin risk engine using live mark-to-market equity and tiered maintenance margin.

use const_decimal::Decimal;
use num_traits::{
    One,
    Zero,
};
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};
use tracing::trace;

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

/// Selects how an exchange computes account equity and liquidation thresholds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MarginMode {
    /// Legacy isolated-margin mode: static entry-price liquidation threshold and
    /// wallet-only available balance (default for backwards compatibility).
    #[default]
    Isolated,
    /// Cross-margin mode: live mark-to-market equity (`wallet_balance + unrealized_pnl`),
    /// mark-notional tiered maintenance margin, and dynamic liquidation pricing.
    Cross,
}

/// Risk engine implementing cross-margin admission and mark-to-market maintenance
/// margin checks.
///
/// Unlike the isolated margin risk engine, this engine:
/// - Evaluates account equity as `wallet_balance + unrealized_pnl(mark_price)`.
/// - Evaluates maintenance margin against live mark notional through the contract's
///   [`crate::risk_engine::MaintenanceMarginSchedule`].
/// - Computes dynamic liquidation prices reflecting actual wallet balance, funding
///   erosion, realized fees, and bracket maintenance offsets.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
pub struct CrossMarginRiskEngine<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    contract_spec: ContractSpecification<I, D, BaseOrQuote>,
}

impl<I, const D: u8, BaseOrQuote> CrossMarginRiskEngine<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    /// Create a new cross-margin risk engine for `contract_spec`.
    #[must_use]
    pub fn new(contract_spec: ContractSpecification<I, D, BaseOrQuote>) -> Self {
        Self { contract_spec }
    }

    /// Compute mark-to-market equity (`wallet_equity + unrealized_pnl`) at `market_state`.
    #[inline]
    #[must_use]
    pub fn mark_to_market_equity<UserOrderIdT: UserOrderId>(
        &self,
        account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
        market_state: &MarketState<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        let upnl = account
            .position()
            .unrealized_pnl_marked(market_state, self.contract_spec.mark_method());
        account.balances().equity() + upnl
    }

    /// Compute required maintenance margin for `position` evaluated at its live mark price.
    #[inline]
    #[must_use]
    pub fn maintenance_margin_requirement(
        &self,
        market_state: &MarketState<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        if position.quantity().is_zero() {
            return Zero::zero();
        }
        let mark_price =
            market_state.mark_price_for_position(self.contract_spec.mark_method(), position.side());
        let mark_notional = position.notional_at_price(mark_price);
        self.contract_spec
            .maintenance_margin_schedule()
            .maintenance_margin(mark_notional)
    }

    /// Check whether mark-to-market equity covers the tiered maintenance margin requirement.
    ///
    /// # Errors
    /// Returns [`RiskError::Liquidate`] when the position is non-zero and
    /// `wallet_equity + unrealized_pnl < maintenance_margin(mark_notional)`.
    #[inline]
    pub fn check_mark_to_market_maintenance_margin<UserOrderIdT: UserOrderId>(
        &self,
        market_state: &MarketState<I, D>,
        account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
    ) -> Result<(), RiskError> {
        if account.position().quantity().is_zero() {
            return Ok(());
        }
        let mtm_equity = self.mark_to_market_equity(account, market_state);
        let mm_req = self.maintenance_margin_requirement(market_state, account.position());
        if mtm_equity < mm_req {
            return Err(RiskError::Liquidate);
        }
        Ok(())
    }

    /// Check whether a new limit order fits within live mark-to-market equity.
    ///
    /// # Errors
    /// Returns [`NotEnoughAvailableBalance`] when `mtm_equity < required_collateral_with_order`.
    #[inline]
    pub fn check_limit_order_with_market<UserOrderIdT: UserOrderId>(
        &self,
        account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
        market_state: &MarketState<I, D>,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), NotEnoughAvailableBalance> {
        let upnl = account
            .position()
            .unrealized_pnl_marked(market_state, self.contract_spec.mark_method());
        let excess = account.margin_excess_with_order(order) + upnl;
        trace!("CrossMarginRiskEngine::check_limit_order_with_market: excess={excess:?}");
        if excess < Zero::zero() {
            return Err(NotEnoughAvailableBalance);
        }
        Ok(())
    }

    /// Check whether a new market order fits within live mark-to-market equity.
    ///
    /// Strictly position-reducing market orders are never rejected.
    ///
    /// # Errors
    /// Returns [`NotEnoughAvailableBalance`] if increasing or flipping the position
    /// would exceed live mark-to-market available margin.
    pub fn check_market_order_with_market<UserOrderIdT: UserOrderId>(
        &self,
        account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
        market_state: &MarketState<I, D>,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
    ) -> Result<(), NotEnoughAvailableBalance> {
        let pos_qty = account.position().quantity();
        let upnl = account
            .position()
            .unrealized_pnl_marked(market_state, self.contract_spec.mark_method());
        let avail = (account.margin_excess() + upnl).max(Zero::zero());

        let same_direction = match order.side() {
            Side::Buy => pos_qty >= Zero::zero(),
            Side::Sell => pos_qty <= Zero::zero(),
        };
        if same_direction {
            let notional = BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
            let init_margin = notional * self.contract_spec.init_margin_req();
            let fee = notional * *self.contract_spec.fee_taker().as_ref();
            if init_margin + fee > avail {
                return Err(NotEnoughAvailableBalance);
            }
            return Ok(());
        }

        let abs_qty = pos_qty.abs();
        if order.quantity() <= abs_qty {
            // Strictly reducing orders are always permitted.
            return Ok(());
        }

        // Flipping position: close old leg (realizing PnL at fill_price) and open remainder.
        let flip_qty = order.quantity() - abs_qty;
        let close_notional = BaseOrQuote::PairedCurrency::convert_from(abs_qty, fill_price);
        let flip_notional = BaseOrQuote::PairedCurrency::convert_from(flip_qty, fill_price);
        let total_fee = (close_notional + flip_notional) * *self.contract_spec.fee_taker().as_ref();
        let realized_pnl =
            BaseOrQuote::PairedCurrency::pnl(account.position().entry_price(), fill_price, pos_qty);
        let post_equity = account.balances().equity() + realized_pnl - total_fee;
        let post_pos_margin = flip_notional * self.contract_spec.init_margin_req();
        if post_equity < post_pos_margin {
            return Err(NotEnoughAvailableBalance);
        }
        Ok(())
    }

    /// Estimate the dynamic liquidation price of a linear or inverse single-position
    /// cross-margin account given `wallet_equity` and `position`.
    ///
    /// Returns `None` if `position` is neutral, or if the liquidation price is non-positive
    /// (for example a long position backed by >= 100% equity with zero leverage).
    ///
    /// For linear contracts (`QuoteCurrency` margin):
    /// - Long (`q > 0`): `E + q * (P - P_e) = q * P * mm - A  =>  P = (q * P_e - E - A) / (q * (1 - mm))`
    /// - Short (`q < 0`, `|q| = Q`): `E + Q * (P_e - P) = Q * P * mm - A  =>  P = (Q * P_e + E + A) / (Q * (1 + mm))`
    ///
    /// For inverse contracts (`BaseCurrency` margin, `notional = Q / P`):
    /// - Long (`q > 0`): `E + q * (1/P_e - 1/P) = (q/P) * mm - A  =>  P = q * (1 + mm) / (E + A + q / P_e)`
    /// - Short (`q < 0`, `Q = |q|`): `E + Q * (1/P - 1/P_e) = (Q/P) * mm - A  =>  P = Q * (1 - mm) / (Q / P_e - E - A)`
    #[must_use]
    pub fn estimated_liquidation_price(
        &self,
        wallet_equity: BaseOrQuote::PairedCurrency,
        position: &Position<I, D, BaseOrQuote>,
        reference_price: QuoteCurrency<I, D>,
    ) -> Option<QuoteCurrency<I, D>> {
        if position.quantity().is_zero() || reference_price <= Zero::zero() {
            return None;
        }
        let schedule = self.contract_spec.maintenance_margin_schedule();
        let tiers = schedule.tiers();
        let side = position.side();
        let qty_abs = *position.quantity().abs().as_ref();
        let entry = *position.entry_price().as_ref();
        let equity = *wallet_equity.as_ref();
        // The bracket applies to the notional AT the liquidation price, not at the
        // reference price. Start at the reference tier and move to the tier of the
        // computed price until both agree. The maintenance curve is continuous and
        // monotone in notional, so the fixed point exists; the step bound guards
        // against rounding ping-pong at a bracket edge.
        let mut idx = schedule.tier_index_for_notional(position.notional_at_price(reference_price));
        let mut liq = None;
        for _ in 0..=tiers.len() {
            let tier = &tiers[idx];
            liq = liquidation_price_for_tier::<I, D, BaseOrQuote::PairedCurrency>(
                side,
                qty_abs,
                entry,
                equity,
                tier.maintenance_margin_rate(),
                *tier.maintenance_amount().as_ref(),
            );
            let price = liq?;
            let liq_idx = schedule.tier_index_for_notional(position.notional_at_price(price));
            if liq_idx == idx {
                break;
            }
            idx = liq_idx;
        }
        if let Some(price) = liq {
            assert!(price > Zero::zero());
        }
        liq
    }
}

/// Liquidation price of a single position under one maintenance tier (`mm`, `cum`),
/// see [`CrossMarginRiskEngine::estimated_liquidation_price`] for the formulas.
/// Returns `None` if the price is non-positive or undefined.
fn liquidation_price_for_tier<I, const D: u8, Margin>(
    side: PositionSide,
    qty_abs: Decimal<I, D>,
    entry: Decimal<I, D>,
    equity: Decimal<I, D>,
    mm: Decimal<I, D>,
    cum: Decimal<I, D>,
) -> Option<QuoteCurrency<I, D>>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    assert2::debug_assert!(qty_abs > Decimal::zero());
    assert2::debug_assert!(cum >= Decimal::zero());
    let liq_dec: Decimal<I, D> = match Margin::KIND {
        crate::types::CurrencyKind::Quote => {
            // Linear: qty in Base, margin/equity in Quote
            match side {
                PositionSide::Long => {
                    let denom = qty_abs * (Decimal::one() - mm);
                    if denom <= Decimal::zero() {
                        return None;
                    }
                    (qty_abs * entry - equity - cum) / denom
                }
                PositionSide::Short => {
                    let denom = qty_abs * (Decimal::one() + mm);
                    if denom <= Decimal::zero() {
                        return None;
                    }
                    (qty_abs * entry + equity + cum) / denom
                }
                PositionSide::Neutral => return None,
            }
        }
        crate::types::CurrencyKind::Base => {
            // Inverse: qty in Quote, margin/equity in Base
            let entry_base = qty_abs / entry;
            match side {
                PositionSide::Long => {
                    let denom = equity + cum + entry_base;
                    if denom <= Decimal::zero() {
                        return None;
                    }
                    (qty_abs * (Decimal::one() + mm)) / denom
                }
                PositionSide::Short => {
                    let denom = entry_base - equity - cum;
                    if denom <= Decimal::zero() {
                        return None;
                    }
                    (qty_abs * (Decimal::one() - mm)) / denom
                }
                PositionSide::Neutral => return None,
            }
        }
    };
    (liq_dec > Decimal::zero()).then(|| QuoteCurrency::from(liq_dec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DECIMALS,
        leverage,
        prelude::*,
        test_fee_maker,
        test_fee_taker,
    };

    fn engine<BaseOrQuote>(
        brackets: &[(Option<BaseOrQuote::PairedCurrency>, Decimal<i64, DECIMALS>)],
    ) -> CrossMarginRiskEngine<i64, DECIMALS, BaseOrQuote>
    where
        BaseOrQuote: Currency<i64, DECIMALS>,
        BaseOrQuote::PairedCurrency: MarginCurrency<i64, DECIMALS>,
    {
        let spec = ContractSpecification::new(
            leverage!(2),
            Decimal::try_from_scaled(5, 1).unwrap(),
            PriceFilter::default(),
            QuantityFilter::default(),
            test_fee_maker(),
            test_fee_taker(),
        )
        .unwrap()
        .with_maintenance_margin_schedule(
            MaintenanceMarginSchedule::from_brackets(brackets).unwrap(),
        )
        .unwrap();
        CrossMarginRiskEngine::new(spec)
    }

    fn rate(scaled: i64, scale: u8) -> Decimal<i64, DECIMALS> {
        Decimal::try_from_scaled(scaled, scale).unwrap()
    }

    #[test]
    fn linear_long_liquidation_price_drops_into_lower_tier() {
        // Reference notional 1000 is in tier 2 (20%, amount 75), but the tier-2 price
        // 37.5 has notional 375 (tier 1). Tier 1: P = (1000 - 625) / (10 * 0.95).
        let re = engine::<BaseCurrency<i64, DECIMALS>>(&[
            (Some(QuoteCurrency::new(500, 0)), rate(5, 2)),
            (None, rate(2, 1)),
        ]);
        let pos = Position::new(BaseCurrency::new(10, 0), QuoteCurrency::new(100, 0)).unwrap();
        let liq = re
            .estimated_liquidation_price(
                QuoteCurrency::new(625, 0),
                &pos,
                QuoteCurrency::new(100, 0),
            )
            .unwrap();
        assert_eq!(liq, QuoteCurrency::new(3_947_368, 5));
        assert!(pos.notional_at_price(liq) <= QuoteCurrency::new(500, 0));
    }

    #[test]
    fn linear_short_liquidation_price_rises_into_higher_tier() {
        // Reference notional 400 is in tier 1, the tier-1 price 130.95 has notional 523.8
        // (tier 2). Tier 2: P = (400 + 150 + 75) / (4 * 1.2) = 130.208333.
        let re = engine::<BaseCurrency<i64, DECIMALS>>(&[
            (Some(QuoteCurrency::new(500, 0)), rate(5, 2)),
            (None, rate(2, 1)),
        ]);
        let pos = Position::new(-BaseCurrency::new(4, 0), QuoteCurrency::new(100, 0)).unwrap();
        let liq = re
            .estimated_liquidation_price(
                QuoteCurrency::new(150, 0),
                &pos,
                QuoteCurrency::new(100, 0),
            )
            .unwrap();
        assert_eq!(liq, QuoteCurrency::new(13_020_833, 5));
        assert!(pos.notional_at_price(liq) > QuoteCurrency::new(500, 0));
    }

    #[test]
    fn inverse_long_liquidation_price_rises_into_higher_tier() {
        // Notional = 300 / P grows as the price falls. Reference notional 3 BTC is tier 1,
        // the tier-1 price 78.75 has notional 3.81 (tier 2, 20%, amount 0.525).
        // Tier 2: P = 300 * 1.2 / (1 + 0.525 + 3) = 79.55801.
        let re = engine::<QuoteCurrency<i64, DECIMALS>>(&[
            (Some(BaseCurrency::new(35, 1)), rate(5, 2)),
            (None, rate(2, 1)),
        ]);
        let pos = Position::new(QuoteCurrency::new(300, 0), QuoteCurrency::new(100, 0)).unwrap();
        let liq = re
            .estimated_liquidation_price(BaseCurrency::new(1, 0), &pos, QuoteCurrency::new(100, 0))
            .unwrap();
        assert_eq!(liq, QuoteCurrency::new(7_955_801, 5));
        assert!(pos.notional_at_price(liq) > BaseCurrency::new(35, 1));
    }

    #[test]
    fn inverse_short_liquidation_price_drops_into_lower_tier() {
        // Reference notional 3 BTC is tier 2 (20%, amount 0.375), the tier-2 price 147.69
        // has notional 2.03 (tier 1). Tier 1: P = 300 * 0.95 / (3 - 1) = 142.5.
        let re = engine::<QuoteCurrency<i64, DECIMALS>>(&[
            (Some(BaseCurrency::new(25, 1)), rate(5, 2)),
            (None, rate(2, 1)),
        ]);
        let pos = Position::new(-QuoteCurrency::new(300, 0), QuoteCurrency::new(100, 0)).unwrap();
        let liq = re
            .estimated_liquidation_price(BaseCurrency::new(1, 0), &pos, QuoteCurrency::new(100, 0))
            .unwrap();
        assert_eq!(liq, QuoteCurrency::new(1425, 1));
        assert!(pos.notional_at_price(liq) <= BaseCurrency::new(25, 1));
    }
}
