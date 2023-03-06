use std::ops::{Add, Div, Mul, Rem, Sub};

use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use fpdec::Decimal;
use serde::{Deserialize, Serialize};

use super::MarginCurrency;
use crate::types::{BaseCurrency, Currency, Fee};

/// Allows the quick construction of `QuoteCurrency`
#[macro_export]
macro_rules! quote {
    ( $a:literal ) => {{
        QuoteCurrency::new(fpdec::Dec!($a))
    }};
}

/// The markets QUOTE currency, e.g.: BTCUSD -> USD is the quote currency
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
    Serialize,
    Deserialize,
)]
#[mul(forward)]
#[div(forward)]
pub struct QuoteCurrency(Decimal);

impl Currency for QuoteCurrency {
    type PairedCurrency = BaseCurrency;

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
        BaseCurrency::new(self.0 / rate.0)
    }

    #[inline(always)]
    fn into_negative(self) -> Self {
        Self(-self.0)
    }
}

impl MarginCurrency for QuoteCurrency {
    /// This represents a linear futures contract pnl calculation
    fn pnl<S>(
        entry_price: QuoteCurrency,
        exit_price: QuoteCurrency,
        quantity: S,
    ) -> S::PairedCurrency
    where
        S: Currency,
    {
        if quantity.is_zero() {
            return S::PairedCurrency::new_zero();
        }
        quantity.convert(exit_price) - quantity.convert(entry_price)
    }
}

/// ### Arithmetic with `Rational` on the right hand side
impl Add<Decimal> for QuoteCurrency {
    type Output = Self;

    fn add(self, rhs: Decimal) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<Decimal> for QuoteCurrency {
    type Output = Self;

    fn sub(self, rhs: Decimal) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Mul<Decimal> for QuoteCurrency {
    type Output = Self;

    fn mul(self, rhs: Decimal) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Div<Decimal> for QuoteCurrency {
    type Output = Self;

    fn div(self, rhs: Decimal) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl Rem for QuoteCurrency {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn quote_display() {
        println!("{}", quote!(0.5));
    }

    #[test]
    fn linear_futures_pnl() {
        assert_eq!(QuoteCurrency::pnl(quote!(100.0), quote!(110.0), base!(10.0)), quote!(100.0));
        assert_eq!(QuoteCurrency::pnl(quote!(100.0), quote!(110.0), base!(-10.0)), quote!(-100.0));
        assert_eq!(QuoteCurrency::pnl(quote!(100.0), quote!(90.0), base!(10.0)), quote!(-100.0));
        assert_eq!(QuoteCurrency::pnl(quote!(100.0), quote!(90.0), base!(-10.0)), quote!(100.0));
    }
}
