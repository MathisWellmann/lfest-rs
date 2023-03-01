use malachite::Rational;

use crate::{Fee, QuoteCurrency};

/// Every unit of account must implement this trait
pub trait Currency:
    Clone
    + Send
    + Sized
    + std::fmt::Debug
    + std::fmt::Display
    // Require to do arithmetic with `Self` on the right hand side
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
    // Require to do arithmetic with `Rational` on the right hand side
    + std::ops::Add<Rational, Output = Self>
    + std::ops::Sub<Rational, Output = Self>
    + std::ops::Mul<Rational, Output = Self>
    + std::ops::Div<Rational, Output = Self>
    // Require to do arithmetic with `&Self` on the right hand side
    + for<'a> std::ops::Add<&'a Self, Output = Self>
    + for<'a> std::ops::Sub<&'a Self, Output = Self>
    + for<'a> std::ops::Mul<&'a Self, Output = Self>
    + for<'a> std::ops::Div<&'a Self, Output = Self>
    + std::ops::AddAssign
    + std::ops::SubAssign
    + for<'a> std::ops::AddAssign<&'a Self>
    + for<'a> std::ops::SubAssign<&'a Self>
    + PartialEq
    + PartialOrd
{
    /// The paired currency.
    /// e.g.: for the BTCUSD market the BTC currency is paired with USD, so the
    /// `PairedCurrency` would be USD
    type PairedCurrency: Currency<PairedCurrency = Self>;

    /// Create a new instance from a `Rational` value
    #[must_use]
    fn new(val: Rational) -> Self;

    /// Create a new instance from a `f64` value
    #[must_use]
    fn from_f64(val: f64) -> Self;

    /// Create a new currency instance with zero value
    #[must_use]
    fn new_zero() -> Self;

    /// Check if the value is zero
    fn is_zero(&self) -> bool;

    /// Check if the value is finite
    #[deprecated] // TODO: See if this method can be removed
    fn is_finite(&self) -> bool;

    /// TODO: it may be smart to remove this here and use another type that can
    /// be negative Get the absolute value
    fn abs(self) -> Self;

    /// Compute the fee denoted in the currency
    fn fee_portion(&self, fee: &Fee) -> Self;

    /// Convert this `Currency`'s value into its pair at the conversion `rate`.
    /// E.g:
    /// 1 BTC @ 20_000 USD means that 1 USD = 1 / 20_000 BTC
    fn convert(&self, rate: &QuoteCurrency) -> Self::PairedCurrency;

    /// Convert the Currency to a negative value
    fn into_negative(self) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, quote, BaseCurrency, QuoteCurrency};

    #[test]
    fn conversion() {
        assert_eq!(base!(1.0).convert(&quote!(20_000.0)), quote!(20_000.0));
        assert_eq!(base!(0.5).convert(&quote!(20_000.0)), quote!(10_000.0));
        assert_eq!(base!(0.25).convert(&quote!(20_000.0)), quote!(5_000.0));

        assert_eq!(quote!(20_000.0).convert(&quote!(20_000.0)), base!(1.0));
        assert_eq!(quote!(10_000.0).convert(&quote!(20_000.0)), base!(0.5));
        assert_eq!(quote!(5_000.0).convert(&quote!(20_000.0)), base!(0.25));
    }
}
