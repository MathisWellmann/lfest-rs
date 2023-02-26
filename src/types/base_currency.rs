use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use malachite::{num::arithmetic::traits::Abs, Rational};

use crate::{Currency, Fee, QuoteCurrency};

/// Allows the quick construction on `BaseCurrency`
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

impl BaseCurrency {
    #[inline(always)]
    pub(crate) fn inner(&self) -> &Rational {
        &self.0
    }
}

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
    fn new_zero() -> Self {
        Self::from_f64(0.0)
    }

    #[inline(always)]
    fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    #[inline(always)]
    fn is_finite(&self) -> bool {
        self.0.is_finite()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_display() {
        println!("{}", base!(0.5));
    }
}
