use std::ops::Mul;

use derive_more::Display;
use fpdec::{Dec, Decimal};

use super::{BaseCurrency, Currency, QuoteCurrency};

/// Fee as a part per one hundred thousand.
#[derive(Default, Debug, Clone, Copy, PartialEq, Display)]
pub struct Fee {
    /// A per cent mill or pcm is one one-thousandth of a percent.
    /// 2.5 basis points would be 25 pcm.
    per_cent_mille: i32,
}

impl Fee {
    /// Create a new instance from a value denoted as a basis point (1 / 10_000)
    #[inline(always)]
    pub const fn from_basis_points(basis_points: i32) -> Self {
        Self {
            per_cent_mille: basis_points * 10,
        }
    }

    /// Create a new instance from a value denoted as (1 / 100_000)
    #[inline(always)]
    pub const fn from_per_cent_mille(pcm: i32) -> Self {
        Self {
            per_cent_mille: pcm,
        }
    }
}

impl<C> Mul<C> for Fee
where
    C: Currency,
{
    type Output = QuoteCurrency;

    fn mul(self, rhs: C) -> Self::Output {
        QuoteCurrency::new(self.per_cent_mille * rhs.as_ref() / Dec!(100000))
    }
}

impl Mul<Fee> for BaseCurrency {
    type Output = Self;

    fn mul(self, rhs: Fee) -> Self::Output {
        BaseCurrency::new(self.as_ref() * Decimal::from(rhs.per_cent_mille) / Dec!(100000))
    }
}

impl Mul<Fee> for QuoteCurrency {
    type Output = Self;

    fn mul(self, rhs: Fee) -> Self::Output {
        QuoteCurrency::new(self.as_ref() * Decimal::from(rhs.per_cent_mille) / Dec!(100000))
    }
}

/// The two types of fees in the maker-taker model.
#[derive(Debug, Clone)]
pub enum FeeType {
    /// The fee limit orders pay.
    Maker(Fee),
    /// The fee market orders pay.
    Taker(Fee),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_fee() {
        assert_eq!(std::mem::size_of::<Fee>(), 4);
    }
}
