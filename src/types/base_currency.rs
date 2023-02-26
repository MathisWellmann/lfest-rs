use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use malachite::Rational;

use crate::{Currency, Fee, QuoteCurrency};

/// Allows the quick construction on `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:expr ) => {{
        BaseCurrency::new($a)
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

    #[inline]
    fn new(val: f64) -> Self {
        Self(Rational::try_from_float_simplest(val).expect("Unable to get Rational from float"))
    }

    #[inline(always)]
    fn new_zero() -> Self {
        Self::new(0.0)
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
        let f: f64 = fee.into();
        Self(self.0 * f)
    }

    #[inline(always)]
    fn convert(&self, rate: QuoteCurrency) -> Self::PairedCurrency {
        let r: f64 = rate.into();
        QuoteCurrency(self.0 * r)
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
