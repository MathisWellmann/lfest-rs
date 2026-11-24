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

use crate::types::{
    ConfigError,
    MarginCurrency,
    Mon,
};

/// A single bracket in a [`MaintenanceMarginSchedule`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, CopyGetters)]
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

        for (idx, &(cap, rate)) in brackets.iter().enumerate() {
            if rate <= Decimal::zero() || rate > Decimal::one() {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            if idx > 0 && rate < prev_rate {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            let is_last = idx + 1 == brackets.len();
            if !is_last && cap.is_none() {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            if let Some(c) = cap {
                if c <= Margin::zero() {
                    return Err(ConfigError::InvalidMaintenanceMarginTier);
                }
                if let Some(pc) = prev_cap
                    && c <= pc
                {
                    return Err(ConfigError::InvalidMaintenanceMarginTier);
                }
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
    /// # Errors
    /// Returns [`ConfigError::InvalidMaintenanceMarginTier`] if the slice is empty
    /// or tiers violate ordering invariants.
    pub fn from_tiers(tiers: &[MaintenanceMarginTier<I, D, Margin>]) -> Result<Self, ConfigError> {
        if tiers.is_empty() {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        let mut prev_cap: Option<Margin> = None;
        let mut prev_rate = Decimal::zero();
        for (idx, tier) in tiers.iter().enumerate() {
            if idx > 0 && tier.maintenance_margin_rate < prev_rate {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            let is_last = idx + 1 == tiers.len();
            if !is_last && tier.notional_cap.is_none() {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            if let (Some(pc), Some(c)) = (prev_cap, tier.notional_cap)
                && c <= pc
            {
                return Err(ConfigError::InvalidMaintenanceMarginTier);
            }
            prev_cap = tier.notional_cap;
            prev_rate = tier.maintenance_margin_rate;
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
        assert2::debug_assert!(notional >= Margin::zero());
        for tier in &self.tiers {
            match tier.notional_cap {
                Some(cap) if notional <= cap => return tier,
                None => return tier,
                Some(_) => {}
            }
        }
        self.tiers
            .last()
            .expect("MaintenanceMarginSchedule always has at least one tier")
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
        let raw = notional * tier.maintenance_margin_rate - tier.maintenance_amount;
        raw.max(Margin::zero())
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
}
