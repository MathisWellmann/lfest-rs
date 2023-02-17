use derive_more::{Add, AddAssign, Display, Div, From, Into, Mul, Sub, SubAssign};

use crate::Fee;

/// The markets BASE currency, e.g.: BTCUSD -> BTC is the BASE currency
/// TODO: make inner type private and create getter and setter
/// TODO: make malachite type / generic
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
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
pub struct BaseCurrency(pub f64);

/// The markets QUOTE currency, e.g.: BTCUSD -> USD is the quote currency
/// TODO: make inner type private and create getter and setter
/// TODO: make malachite type / generic
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
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
pub struct QuoteCurrency(pub f64);

pub trait Currency:
    Copy
    + std::fmt::Debug
    + std::fmt::Display
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
    + std::ops::AddAssign
    + std::ops::SubAssign
    + PartialEq
    + PartialOrd
    + Into<f64>
    + From<f64>
{
    /// The paired currency.
    /// e.g.: for the BTCUSD market the BTC currency is paired with USD, so the
    /// `PairedCurrency` would be USD
    type PairedCurrency: Currency<PairedCurrency = Self>;

    /// Create a new currency instance with zero value
    fn new_zero() -> Self;

    /// Check if the value is zero
    fn is_zero(&self) -> bool;

    /// Check if the value is finite
    fn is_finite(&self) -> bool;

    /// TODO: it may be smart to remove this here and use another type that can
    /// be negative Get the absolute value
    fn abs(self) -> Self;

    /// Compute the natural logarithm
    fn ln(&self) -> Self;

    /// Compute the fee denoted in the currency
    fn fee_portion(&self, fee: Fee) -> Self;

    /// Convert this `Currency`'s value into its pair at the conversion `rate`.
    /// E.g:
    /// 1 BTC @ 20_000 USD means that 1 USD = 1 / 20_000 BTC
    fn convert(&self, rate: QuoteCurrency) -> Self::PairedCurrency;

    /// Convert the Currency to a negative value
    fn into_negative(self) -> Self;

    /// Convert into a rounded value with the given precision of decimal prices
    #[cfg(test)]
    fn into_rounded(self, prec: i32) -> Self;
}

impl Currency for BaseCurrency {
    type PairedCurrency = QuoteCurrency;

    #[inline(always)]
    fn new_zero() -> Self {
        Self(0.0)
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
    fn ln(&self) -> Self {
        Self(self.0.ln())
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

    #[cfg(test)]
    fn into_rounded(self, prec: i32) -> Self {
        Self(crate::utils::round(self.0, prec))
    }
}

impl Currency for QuoteCurrency {
    type PairedCurrency = BaseCurrency;

    #[inline(always)]
    fn new_zero() -> Self {
        Self(0.0)
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
    fn ln(&self) -> Self {
        Self(self.0.ln())
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

    #[cfg(test)]
    fn into_rounded(self, prec: i32) -> Self {
        Self(crate::utils::round(self.0, prec))
    }
}

/// Allows the quick construction on `QuoteCurrency`
#[macro_export]
macro_rules! quote {
    ( $a:expr ) => {{
        QuoteCurrency($a)
    }};
}

/// Allows the quick construction on `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:expr ) => {{
        BaseCurrency($a)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_display() {
        println!("{}", base!(0.5));
    }

    fn conversion() {
        assert_eq!(BaseCurrency(1.0).convert(quote!(20_000.0)), QuoteCurrency(20_000.0));
        assert_eq!(BaseCurrency(0.5).convert(quote!(20_000.0)), QuoteCurrency(10_000.0));
        assert_eq!(BaseCurrency(0.25).convert(quote!(20_000.0)), QuoteCurrency(5_000.0));

        assert_eq!(QuoteCurrency(20_000.0).convert(quote!(20_000.0)), BaseCurrency(1.0));
        assert_eq!(QuoteCurrency(10_000.0).convert(quote!(20_000.0)), BaseCurrency(0.5));
        assert_eq!(QuoteCurrency(5_000.0).convert(quote!(20_000.0)), BaseCurrency(0.5));
    }
}
