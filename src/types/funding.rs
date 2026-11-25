//! Perpetual futures funding rate newtype.

use std::fmt;

use const_decimal::Decimal;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use super::Mon;

/// A signed perpetual futures funding rate expressed as a decimal fraction
/// (for example `0.0001` = `+1 bp` = `Decimal::try_from_scaled(1, 4)`).
///
/// Sign convention:
/// - Positive (`> 0`): longs pay shorts.
/// - Negative (`< 0`): shorts pay longs.
/// - Zero (`0`): no funding is exchanged.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(transparent)]
pub struct FundingRate<I, const D: u8>(Decimal<I, D>);

impl<I: Copy, const D: u8> FundingRate<I, D> {
    /// Return the underlying [`Decimal`] funding rate fraction.
    #[inline(always)]
    #[must_use]
    pub const fn into_inner(self) -> Decimal<I, D> {
        self.0
    }
}

impl<I, const D: u8> From<Decimal<I, D>> for FundingRate<I, D>
where
    I: Mon<D>,
{
    #[inline(always)]
    fn from(value: Decimal<I, D>) -> Self {
        Self(value)
    }
}

impl<I, const D: u8> AsRef<Decimal<I, D>> for FundingRate<I, D> {
    #[inline(always)]
    fn as_ref(&self) -> &Decimal<I, D> {
        &self.0
    }
}

impl<I, const D: u8> fmt::Display for FundingRate<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_funding_rate() {
        assert_eq!(size_of::<FundingRate<i32, 5>>(), 4);
        assert_eq!(size_of::<FundingRate<i64, 5>>(), 8);
        assert_eq!(size_of::<FundingRate<i64, 8>>(), 8);
    }

    #[test]
    fn funding_rate_conversions_and_display() {
        let pos = FundingRate::from(Decimal::<i64, 5>::try_from_scaled(1, 4).unwrap());
        let neg = FundingRate::from(Decimal::<i64, 5>::try_from_scaled(-25, 5).unwrap());
        assert_eq!(*pos.as_ref(), Decimal::try_from_scaled(1, 4).unwrap());
        assert_eq!(pos.into_inner(), Decimal::try_from_scaled(1, 4).unwrap());
        assert_eq!(pos.to_string(), "0.00010");
        assert_eq!(neg.to_string(), "-0.00025");
        assert!(neg < FundingRate::default());
        assert!(pos > FundingRate::default());
    }
}
