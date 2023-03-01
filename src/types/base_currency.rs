use std::ops::{Add, Div, Mul, Sub};

use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use malachite::{
    num::{arithmetic::traits::Abs, basic::traits::Zero},
    Rational,
};

use crate::{Currency, Fee, QuoteCurrency};

/// Allows the quick construction of `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:expr ) => {{
        BaseCurrency::from_f64($a)
    }};
}

/// The markets BASE currency, e.g.: BTCUSD -> BTC is the BASE currency
#[derive(
    Default,
    Debug,
    Clone,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
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
pub struct BaseCurrency(Rational);

impl Currency for BaseCurrency {
    type PairedCurrency = QuoteCurrency;

    #[inline(always)]
    fn new(val: Rational) -> Self {
        Self(val)
    }

    #[inline]
    fn from_f64(val: f64) -> Self {
        Self(Rational::try_from_float_simplest(val).expect("Unable to get Rational from float"))
    }

    #[inline(always)]
    fn inner(self) -> Rational {
        self.0
    }

    #[inline(always)]
    fn inner_ref(&self) -> &Rational {
        &self.0
    }

    #[inline(always)]
    fn new_zero() -> Self {
        Self::new(Rational::ZERO)
    }

    #[inline(always)]
    fn is_zero(&self) -> bool {
        self.0.eq(&Rational::ZERO)
    }

    #[inline(always)]
    fn is_finite(&self) -> bool {
        // self.0.is_finite()
        todo!()
    }

    #[inline(always)]
    fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[inline(always)]
    fn fee_portion(&self, fee: &Fee) -> Self {
        Self(&self.0 * fee.inner_ref())
    }

    #[inline(always)]
    fn convert(&self, rate: &QuoteCurrency) -> Self::PairedCurrency {
        QuoteCurrency::new(&self.0 * rate.inner_ref())
    }

    #[inline(always)]
    fn into_negative(self) -> Self {
        Self(-self.0)
    }
}

/// ### Arithmetic with `Rational` on the right hand side
impl Add<Rational> for BaseCurrency {
    type Output = Self;

    fn add(self, rhs: Rational) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl<'a> Add<&'a Rational> for BaseCurrency {
    type Output = Self;

    fn add(self, rhs: &'a Rational) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<Rational> for BaseCurrency {
    type Output = Self;

    fn sub(self, rhs: Rational) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl<'a> Sub<&'a Rational> for BaseCurrency {
    type Output = Self;

    fn sub(self, rhs: &'a Rational) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Mul<Rational> for BaseCurrency {
    type Output = Self;

    fn mul(self, rhs: Rational) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl<'a> Mul<&'a Rational> for BaseCurrency {
    type Output = Self;

    fn mul(self, rhs: &'a Rational) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Div<Rational> for BaseCurrency {
    type Output = Self;

    fn div(self, rhs: Rational) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl<'a> Div<&'a Rational> for BaseCurrency {
    type Output = Self;

    fn div(self, rhs: &'a Rational) -> Self::Output {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_display() {
        println!("{}", base!(0.5));
    }
}
