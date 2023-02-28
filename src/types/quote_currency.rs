use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use malachite::{
    num::{arithmetic::traits::Abs, basic::traits::Zero},
    Rational,
};

use crate::{BaseCurrency, Currency, Fee};

/// Allows the quick construction of `QuoteCurrency`
#[macro_export]
macro_rules! quote {
    ( $a:expr ) => {{
        QuoteCurrency::from_f64($a)
    }};
}

/// The markets QUOTE currency, e.g.: BTCUSD -> USD is the quote currency
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
pub struct QuoteCurrency(Rational);

impl Currency for QuoteCurrency {
    type PairedCurrency = BaseCurrency;

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
        Self(self.0 * fee.inner())
    }
    #[inline(always)]
    fn convert(&self, rate: &QuoteCurrency) -> Self::PairedCurrency {
        BaseCurrency::new(self.0 / rate.inner())
    }

    #[inline(always)]
    fn into_negative(self) -> Self {
        Self(-self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_display() {
        println!("{}", quote!(0.5));
    }
}
