use std::ops::{Add, Div, Mul, Sub};

use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use fpdec::Decimal;

use crate::types::{Currency, Fee, QuoteCurrency};

/// Allows the quick construction of `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:literal ) => {{
        BaseCurrency::new(fpdec::Dec!($a))
    }};
}

/// The markets BASE currency, e.g.: BTCUSD -> BTC is the BASE currency
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Add,
    Sub,
    Mul,
    Div,
    AddAssign,
    SubAssign,
    Display,
    Into,
    From,
)]
#[mul(forward)]
#[div(forward)]
pub struct BaseCurrency(Decimal);

impl Currency for BaseCurrency {
    type PairedCurrency = QuoteCurrency;

    #[inline(always)]
    fn new(val: Decimal) -> Self {
        Self(val)
    }

    #[inline(always)]
    fn inner(self) -> Decimal {
        self.0
    }

    #[inline(always)]
    fn new_zero() -> Self {
        Self::new(Decimal::ZERO)
    }

    #[inline(always)]
    fn is_zero(&self) -> bool {
        self.0.eq(&Decimal::ZERO)
    }

    #[inline(always)]
    fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[inline(always)]
    fn fee_portion(&self, fee: Fee) -> Self {
        Self(self.0 * fee.inner())
    }

    #[inline(always)]
    fn convert(&self, rate: QuoteCurrency) -> Self::PairedCurrency {
        QuoteCurrency::new(self.0 * rate.inner())
    }

    #[inline(always)]
    fn into_negative(self) -> Self {
        Self(-self.0)
    }
}

/// ### Arithmetic with `Decimal` on the right hand side
impl Add<Decimal> for BaseCurrency {
    type Output = Self;

    fn add(self, rhs: Decimal) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<Decimal> for BaseCurrency {
    type Output = Self;

    fn sub(self, rhs: Decimal) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Mul<Decimal> for BaseCurrency {
    type Output = Self;

    fn mul(self, rhs: Decimal) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Div<Decimal> for BaseCurrency {
    type Output = Self;

    fn div(self, rhs: Decimal) -> Self::Output {
        Self(self.0 / rhs)
    }
}

/// ### Arithmetic with `&Self` on the right hand side
impl<'a> Add<&'a Self> for BaseCurrency {
    type Output = Self;

    fn add(self, rhs: &'a Self) -> Self::Output {
        Self(self.0 + &rhs.0)
    }
}

impl<'a> Sub<&'a Self> for BaseCurrency {
    type Output = Self;

    fn sub(self, rhs: &'a Self) -> Self::Output {
        Self(self.0 - &rhs.0)
    }
}

impl<'a> Mul<&'a Self> for BaseCurrency {
    type Output = Self;

    fn mul(self, rhs: &'a Self) -> Self::Output {
        Self(self.0 * &rhs.0)
    }
}

impl<'a> Div<&'a Self> for BaseCurrency {
    type Output = Self;

    fn div(self, rhs: &'a Self) -> Self::Output {
        Self(self.0 / &rhs.0)
    }
}

/// ### Arithmetic assignment with `&Self` on the right hand side
impl<'a> std::ops::AddAssign<&'a Self> for BaseCurrency {
    fn add_assign(&mut self, rhs: &'a Self) {
        self.0 = &self.0 + &rhs.0;
    }
}

impl<'a> std::ops::SubAssign<&'a Self> for BaseCurrency {
    fn sub_assign(&mut self, rhs: &'a Self) {
        self.0 = &self.0 - &rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_display() {
        println!("{}", base!(0.5));
    }
}
