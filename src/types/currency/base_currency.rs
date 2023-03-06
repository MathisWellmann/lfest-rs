use std::ops::{Add, Div, Mul, Rem, Sub};

use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};
use fpdec::Decimal;

use super::MarginCurrency;
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

impl MarginCurrency for BaseCurrency {
    /// This represents the pnl calculation for inverse futures contracts
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
        quantity.convert(entry_price) - quantity.convert(exit_price)
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

impl Rem for BaseCurrency {
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
    fn base_display() {
        println!("{}", base!(0.5));
    }

    #[test]
    fn inverse_futures_pnl() {
        assert_eq!(BaseCurrency::pnl(quote!(100.0), quote!(125.0), quote!(1000.0)), base!(2.0));
        assert_eq!(BaseCurrency::pnl(quote!(100.0), quote!(125.0), quote!(-1000.0)), base!(-2.0));
        assert_eq!(BaseCurrency::pnl(quote!(100.0), quote!(80.0), quote!(1000.0)), base!(-2.5));
        assert_eq!(BaseCurrency::pnl(quote!(100.0), quote!(80.0), quote!(-1000.0)), base!(2.5));
    }
}
