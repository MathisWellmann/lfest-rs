use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use malachite::Rational;

use crate::{BaseCurrency, Currency, Fee};

/// Allows the quick construction on `QuoteCurrency`
#[macro_export]
macro_rules! quote {
    ( $a:expr ) => {{
        QuoteCurrency::new($a)
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

    /// Create a new instance from an f64 value
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
        BaseCurrency(self.0 / r)
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
