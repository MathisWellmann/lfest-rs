//! Tiered maintenance margin bracket schedule (Binance / Bybit piecewise-continuous model).
//!
//! Real perpetual futures venues raise maintenance margin rates for larger
//! notional positions while keeping the required maintenance margin continuous
//! across bracket boundaries via a cumulative `maintenance_amount` offset:
//!
//! `maintenance_margin(notional) = notional * maintenance_margin_rate - maintenance_amount`.

use const_decimal::Decimal;
use getset::CopyGetters;
use num_traits::{
    One,
    Zero,
};
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use crate::types::{
    ConfigError,
    MarginCurrency,
    Mon,
};

/// A single bracket in a [`MaintenanceMarginSchedule`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, CopyGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MaintenanceMarginTier<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Upper notional bound (inclusive) for this tier in margin currency.
    /// `None` denotes an unbounded top tier.
    #[getset(get_copy = "pub")]
    notional_cap: Option<Margin>,

    /// Maintenance margin rate applied to notional in this tier (in `(0, 1]`).
    #[getset(get_copy = "pub")]
    maintenance_margin_rate: Decimal<I, D>,

    /// Cumulative continuity offset subtracted so the maintenance margin curve
    /// is continuous at bracket boundaries (`>= 0`).
    #[getset(get_copy = "pub")]
    maintenance_amount: Margin,

    _i: std::marker::PhantomData<I>,
}

impl<I, const D: u8, Margin> MaintenanceMarginTier<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Create a validated tier with an explicit continuity offset `maintenance_amount`.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidMaintenanceMarginTier`] if
    /// `maintenance_margin_rate` is outside `(0, 1]`, `notional_cap <= 0`, or
    /// `maintenance_amount < 0`.
    pub fn new(
        notional_cap: Option<Margin>,
        maintenance_margin_rate: Decimal<I, D>,
        maintenance_amount: Margin,
    ) -> Result<Self, ConfigError> {
        if maintenance_margin_rate <= Decimal::zero() || maintenance_margin_rate > Decimal::one() {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        if let Some(cap) = notional_cap
            && cap <= Margin::zero()
        {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        if maintenance_amount < Margin::zero() {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        Ok(Self {
            notional_cap,
            maintenance_margin_rate,
            maintenance_amount,
            _i: std::marker::PhantomData,
        })
    }
}

/// Ordered tiered maintenance margin schedule for a contract.
///
/// Tiers are sorted in strictly ascending `notional_cap` order with non-decreasing
/// `maintenance_margin_rate`, and the final tier covers unbounded notionals
/// (either `notional_cap: None`, or if capped, notionals above the top cap use
/// the top tier's rate).
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MaintenanceMarginSchedule<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    tiers: Box<[MaintenanceMarginTier<I, D, Margin>]>,
}

impl<I, const D: u8, Margin> MaintenanceMarginSchedule<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Construct a single flat-rate schedule with zero continuity offset.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidMaintenanceMarginTier`] if `rate` is outside `(0, 1]`.
    pub fn flat(rate: Decimal<I, D>) -> Result<Self, ConfigError> {
        let tier = MaintenanceMarginTier::new(None, rate, Margin::zero())?;
        Ok(Self {
            tiers: Box::from([tier]),
        })
    }

    /// Construct a piecewise-continuous schedule from `(notional_cap, maintenance_margin_rate)`
    /// pairs, automatically computing each tier's continuity offset `maintenance_amount`:
    ///
    /// `cum_i = cum_{i-1} + cap_{i-1} * (rate_i - rate_{i-1})`.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidMaintenanceMarginTier`] if:
    /// - `brackets` is empty,
    /// - any rate is outside `(0, 1]` or decreases relative to the previous tier,
    /// - any non-final tier has `notional_cap == None`,
    /// - any `notional_cap` is non-positive or not strictly increasing.
    pub fn from_brackets(
        brackets: &[(Option<Margin>, Decimal<I, D>)],
    ) -> Result<Self, ConfigError> {
        if brackets.is_empty() {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        let mut tiers = Vec::with_capacity(brackets.len());
        let mut prev_cap: Option<Margin> = None;
        let mut prev_rate = Decimal::zero();
        let mut prev_amount = Margin::zero();

        // `MaintenanceMarginTier::new` validates each rate and cap on its own, so only
        // the relations between consecutive tiers are checked here. The first tier
        // compares against a `prev_rate` of zero, which `new` rejects anyway.
        for (idx, &(cap, rate)) in brackets.iter().enumerate() {
            if rate < prev_rate {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            let is_last = idx + 1 == brackets.len();
            if !is_last && cap.is_none() {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            if let (Some(c), Some(pc)) = (cap, prev_cap)
                && c <= pc
            {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            let amount = if let Some(pc) = prev_cap {
                prev_amount + pc * (rate - prev_rate)
            } else {
                Margin::zero()
            };
            let tier = MaintenanceMarginTier::new(cap, rate, amount)?;
            tiers
                .push_within_capacity(tier)
                .expect("capacity equals brackets.len()");
            prev_cap = cap;
            prev_rate = rate;
            prev_amount = amount;
        }

        Ok(Self {
            tiers: tiers.into_boxed_slice(),
        })
    }

    /// Construct a schedule from explicit [`MaintenanceMarginTier`] entries.
    ///
    /// The offsets must make the maintenance margin curve continuous, otherwise the
    /// requirement jumps (or turns negative) at a bracket boundary:
    /// `amount_0 == 0` and `amount_i == amount_{i-1} + cap_{i-1} * (rate_i - rate_{i-1})`.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidMaintenanceMarginTier`] if the slice is empty,
    /// caps are not strictly increasing, rates decrease, a non-final tier is unbounded,
    /// or any `maintenance_amount` breaks continuity.
    pub fn from_tiers(tiers: &[MaintenanceMarginTier<I, D, Margin>]) -> Result<Self, ConfigError> {
        if tiers.is_empty() {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        let mut prev: Option<&MaintenanceMarginTier<I, D, Margin>> = None;
        for (idx, tier) in tiers.iter().enumerate() {
            let is_last = idx + 1 == tiers.len();
            if !is_last && tier.notional_cap.is_none() {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            let expected_amount = match prev {
                None => Margin::zero(),
                Some(p) => {
                    if tier.maintenance_margin_rate < p.maintenance_margin_rate {
                        return Err(ConfigError::InvalidMaintenanceMarginTier);
                    }
                    let pc = p
                        .notional_cap
                        .expect("only the final tier may be unbounded, checked above");
                    if let Some(c) = tier.notional_cap
                        && c <= pc
                    {
                        return Err(ConfigError::InvalidMaintenanceMarginTier);
                    }
                    p.maintenance_amount
                        + pc * (tier.maintenance_margin_rate - p.maintenance_margin_rate)
                }
            };
            if tier.maintenance_amount != expected_amount {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            prev = Some(tier);
        }
        Ok(Self {
            tiers: Box::from(tiers),
        })
    }

    /// Inspect the configured tiers.
    #[inline]
    #[must_use]
    pub fn tiers(&self) -> &[MaintenanceMarginTier<I, D, Margin>] {
        &self.tiers
    }

    /// Look up the active tier for a non-negative position `notional`.
    #[inline]
    #[must_use]
    pub fn tier_for_notional(&self, notional: Margin) -> &MaintenanceMarginTier<I, D, Margin> {
        &self.tiers[self.tier_index_for_notional(notional)]
    }

    /// Index of the active tier for a non-negative position `notional`.
    #[inline]
    #[must_use]
    pub(crate) fn tier_index_for_notional(&self, notional: Margin) -> usize {
        assert2::debug_assert!(notional >= Margin::zero());
        assert!(!self.tiers.is_empty());
        self.tiers
            .iter()
            .position(|tier| tier.notional_cap.is_none_or(|cap| notional <= cap))
            // Notionals above the top cap use the top tier.
            .unwrap_or(self.tiers.len() - 1)
    }

    /// The highest maintenance margin rate of any tier (the rate of the last tier).
    #[inline]
    #[must_use]
    pub fn max_maintenance_margin_rate(&self) -> Decimal<I, D> {
        self.tiers
            .last()
            .expect("MaintenanceMarginSchedule always has at least one tier")
            .maintenance_margin_rate
    }

    /// The maintenance margin for a positive `notional` expressed as a rate of that notional
    /// (`maintenance_margin(notional) / notional`).
    ///
    /// Returns the tier rate unchanged when the tier has no continuity offset, so a flat
    /// schedule loses no precision to the division.
    #[inline]
    #[must_use]
    pub fn effective_rate(&self, notional: Margin) -> Decimal<I, D> {
        assert2::debug_assert!(notional > Margin::zero());
        let tier = self.tier_for_notional(notional);
        if tier.maintenance_amount.is_zero() {
            return tier.maintenance_margin_rate;
        }
        let rate = *self.maintenance_margin(notional).as_ref() / *notional.as_ref();
        assert2::debug_assert!(rate >= Decimal::zero());
        assert2::debug_assert!(rate <= tier.maintenance_margin_rate);
        rate
    }

    /// Compute the required maintenance margin for a non-negative `notional`.
    ///
    /// Returns zero when `notional == 0`.
    #[inline]
    #[must_use]
    pub fn maintenance_margin(&self, notional: Margin) -> Margin {
        assert2::debug_assert!(notional >= Margin::zero());
        if notional.is_zero() {
            return Margin::zero();
        }
        let tier = self.tier_for_notional(notional);
        let mm = notional * tier.maintenance_margin_rate - tier.maintenance_amount;
        // Continuity (validated at construction) keeps the curve non-negative:
        // `mm(n) = mm(cap_{i-1}) + (n - cap_{i-1}) * rate_i` for `n` inside tier `i`.
        assert!(
            mm >= Margin::zero(),
            "maintenance margin must not be negative"
        );
        mm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::QuoteCurrency;

    #[test]
    fn tiered_schedule_is_continuous_at_boundaries() {
        let r1 = Decimal::<i64, 5>::try_from_scaled(5, 3).unwrap(); // 0.5%
        let r2 = Decimal::<i64, 5>::try_from_scaled(1, 2).unwrap(); // 1.0%
        let r3 = Decimal::<i64, 5>::try_from_scaled(25, 3).unwrap(); // 2.5%

        let sched = MaintenanceMarginSchedule::from_brackets(&[
            (Some(QuoteCurrency::new(10_000, 0)), r1),
            (Some(QuoteCurrency::new(50_000, 0)), r2),
            (None, r3),
        ])
        .unwrap();

        // Tier 1: notional 10_000 -> 10_000 * 0.005 = 50
        assert_eq!(
            sched.maintenance_margin(QuoteCurrency::new(10_000, 0)),
            QuoteCurrency::new(50, 0)
        );
        // Continuity offset for tier 2: 10_000 * (0.01 - 0.005) = 50.
        // Tier 2 at 50_000: 50_000 * 0.01 - 50 = 450.
        assert_eq!(
            sched.tiers()[1].maintenance_amount(),
            QuoteCurrency::new(50, 0)
        );
        assert_eq!(
            sched.maintenance_margin(QuoteCurrency::new(50_000, 0)),
            QuoteCurrency::new(450, 0)
        );
        // Continuity offset for tier 3: 50 + 50_000 * (0.025 - 0.01) = 800.
        // Tier 3 at 100_000: 100_000 * 0.025 - 800 = 1700.
        assert_eq!(
            sched.tiers()[2].maintenance_amount(),
            QuoteCurrency::new(800, 0)
        );
        assert_eq!(
            sched.maintenance_margin(QuoteCurrency::new(100_000, 0)),
            QuoteCurrency::new(1_700, 0)
        );
        // Zero notional always requires zero maintenance margin.
        assert!(sched.maintenance_margin(QuoteCurrency::zero()).is_zero());
    }

    #[test]
    fn invalid_brackets_are_rejected() {
        let r1 = Decimal::<i64, 5>::try_from_scaled(1, 2).unwrap();
        let r2 = Decimal::<i64, 5>::try_from_scaled(5, 3).unwrap();

        // Empty
        assert_eq!(
            MaintenanceMarginSchedule::<i64, 5, QuoteCurrency<i64, 5>>::from_brackets(&[]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
        // Decreasing rate
        assert_eq!(
            MaintenanceMarginSchedule::from_brackets(&[
                (Some(QuoteCurrency::new(10_000, 0)), r1),
                (None, r2),
            ]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
        // Non-increasing cap
        assert_eq!(
            MaintenanceMarginSchedule::from_brackets(&[
                (Some(QuoteCurrency::new(10_000, 0)), r2),
                (Some(QuoteCurrency::new(10_000, 0)), r1),
            ]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
        // Unbounded non-final tier
        assert_eq!(
            MaintenanceMarginSchedule::<i64, 5, QuoteCurrency<i64, 5>>::from_brackets(&[
                (None, r2),
                (None, r1),
            ]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn from_brackets_rejects_rates_outside_the_unit_interval() {
        type S = MaintenanceMarginSchedule<i64, 5, QuoteCurrency<i64, 5>>;
        let cap = Some(QuoteCurrency::new(10_000, 0));

        for rate in [Decimal::zero(), Decimal::try_from_scaled(-1, 3).unwrap()] {
            assert_eq!(
                S::from_brackets(&[(cap, rate)]),
                Err(ConfigError::InvalidMaintenanceMarginTier),
                "rate {rate:?}"
            );
        }

        // 100% is the inclusive upper bound...
        S::from_brackets(&[(cap, Decimal::one())]).unwrap();
        // ...but anything above it is rejected.
        assert_eq!(
            S::from_brackets(&[(cap, Decimal::try_from_scaled(1001, 3).unwrap())]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn from_brackets_rejects_non_positive_caps() {
        let rate = Decimal::<i64, 5>::try_from_scaled(5, 3).unwrap();

        assert_eq!(
            MaintenanceMarginSchedule::from_brackets(&[(Some(QuoteCurrency::zero()), rate)]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
        assert_eq!(
            MaintenanceMarginSchedule::from_brackets(&[(Some(QuoteCurrency::new(-1, 0)), rate)]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn from_brackets_accepts_equal_consecutive_rates() {
        let rate = Decimal::<i64, 5>::try_from_scaled(5, 3).unwrap();

        let sched = MaintenanceMarginSchedule::from_brackets(&[
            (Some(QuoteCurrency::new(10_000, 0)), rate),
            (None, rate),
        ])
        .unwrap();

        // No rate change means no continuity offset.
        assert!(sched.tiers()[1].maintenance_amount().is_zero());
    }

    #[test]
    fn from_brackets_rejects_a_rate_decrease_that_keeps_the_offset_non_negative() {
        // The large previous cap makes `cap * (rate - prev_rate)` smaller than the
        // accumulated offset, so the tier constructor accepts the amount. Only the
        // explicit non-decreasing-rate check can reject this bracket.
        let low = Decimal::<i64, 5>::try_from_scaled(1, 3).unwrap(); // 0.001
        let high = Decimal::<i64, 5>::try_from_scaled(1, 1).unwrap(); // 0.1
        let lower = Decimal::<i64, 5>::try_from_scaled(999, 4).unwrap(); // 0.0999

        assert_eq!(
            MaintenanceMarginSchedule::from_brackets(&[
                (Some(QuoteCurrency::new(10_000, 0)), low),
                (Some(QuoteCurrency::new(1_000_000, 0)), high),
                (Some(QuoteCurrency::new(1_000_001, 0)), lower),
            ]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn tier_lookup_uses_inclusive_caps_and_falls_back_to_the_top_tier() {
        let r1 = Decimal::<i64, 5>::try_from_scaled(5, 3).unwrap();
        let r2 = Decimal::<i64, 5>::try_from_scaled(1, 2).unwrap();
        let sched = MaintenanceMarginSchedule::from_brackets(&[
            (Some(QuoteCurrency::new(10_000, 0)), r1),
            (Some(QuoteCurrency::new(50_000, 0)), r2),
        ])
        .unwrap();

        assert_eq!(
            sched.tier_index_for_notional(QuoteCurrency::new(9_999, 0)),
            0
        );
        // Caps are inclusive.
        assert_eq!(
            sched.tier_index_for_notional(QuoteCurrency::new(10_000, 0)),
            0
        );
        assert_eq!(
            sched.tier_index_for_notional(QuoteCurrency::new(10_001, 0)),
            1
        );
        assert_eq!(
            sched.tier_index_for_notional(QuoteCurrency::new(50_000, 0)),
            1
        );
        // Above the top cap the top tier applies instead of an out-of-bounds index.
        assert_eq!(
            sched.tier_index_for_notional(QuoteCurrency::new(1_000_000, 0)),
            1
        );
    }
}

#[cfg(test)]
mod from_tiers_tests {
    use super::*;
    use crate::prelude::QuoteCurrency;

    type Tier = MaintenanceMarginTier<i64, 5, QuoteCurrency<i64, 5>>;

    fn tier(cap: Option<i64>, rate_permille: i64, amount: i64) -> Tier {
        MaintenanceMarginTier::new(
            cap.map(|c| QuoteCurrency::new(c, 0)),
            Decimal::try_from_scaled(rate_permille, 3).unwrap(),
            QuoteCurrency::new(amount, 0),
        )
        .unwrap()
    }

    #[test]
    fn from_tiers_accepts_continuous_offsets() {
        // amount_1 = 0 + 10_000 * (0.010 - 0.005) = 50.
        let sched =
            MaintenanceMarginSchedule::from_tiers(&[tier(Some(10_000), 5, 0), tier(None, 10, 50)])
                .unwrap();
        assert_eq!(
            sched.maintenance_margin(QuoteCurrency::new(20_000, 0)),
            QuoteCurrency::new(150, 0)
        );
    }

    #[test]
    fn from_tiers_rejects_discontinuous_offsets() {
        let err = Err(ConfigError::InvalidMaintenanceMarginTier);
        // Non-zero first offset.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[tier(None, 5, 1)]),
            err
        );
        // Offset too large: would make the requirement negative just above the boundary.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[tier(Some(10_000), 5, 0), tier(None, 10, 500)]),
            err
        );
        // Offset too small (jump at the boundary), one unit off the continuous value.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[tier(Some(10_000), 5, 0), tier(None, 10, 49)]),
            err
        );
        // Equal caps.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[
                tier(Some(10_000), 5, 0),
                tier(Some(10_000), 10, 50),
            ]),
            err
        );
        // Decreasing rate.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[tier(Some(10_000), 10, 0), tier(None, 5, 0)]),
            err
        );
    }

    #[test]
    fn from_tiers_rejects_empty_and_unbounded_non_final_tiers() {
        assert_eq!(
            MaintenanceMarginSchedule::<i64, 5, QuoteCurrency<i64, 5>>::from_tiers(&[]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[tier(None, 5, 0), tier(None, 10, 0)]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn from_tiers_rejects_decreasing_caps_with_continuous_offsets() {
        // Equal rates keep the offset at zero, so only the cap ordering can reject.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[
                tier(Some(20_000), 5, 0),
                tier(Some(10_000), 5, 0),
            ]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn from_tiers_accepts_equal_consecutive_rates() {
        let sched =
            MaintenanceMarginSchedule::from_tiers(&[tier(Some(10_000), 5, 0), tier(None, 5, 0)])
                .unwrap();

        // No rate change means no continuity offset.
        assert!(sched.tiers()[1].maintenance_amount().is_zero());
    }

    #[test]
    fn from_tiers_rejects_a_rate_decrease_that_keeps_the_offset_non_negative() {
        // The large first cap makes the accumulated offset exceed the drop in the
        // second cap's rate, so every tier constructor accepts its amount. Only the
        // explicit non-decreasing-rate check can reject this schedule.
        assert_eq!(
            MaintenanceMarginSchedule::from_tiers(&[
                tier(Some(100_000), 1, 0),
                tier(Some(1_000_000), 100, 9_900),
                tier(Some(1_000_001), 99, 8_900),
            ]),
            Err(ConfigError::InvalidMaintenanceMarginTier)
        );
    }

    #[test]
    fn effective_rate_reflects_offset() {
        let sched =
            MaintenanceMarginSchedule::from_tiers(&[tier(Some(10_000), 5, 0), tier(None, 10, 50)])
                .unwrap();
        // (20_000 * 0.01 - 50) / 20_000 = 0.0075
        assert_eq!(
            sched.effective_rate(QuoteCurrency::new(20_000, 0)),
            Decimal::try_from_scaled(75, 4).unwrap()
        );
        assert_eq!(
            sched.effective_rate(QuoteCurrency::new(1_000, 0)),
            Decimal::try_from_scaled(5, 3).unwrap()
        );
    }
}
