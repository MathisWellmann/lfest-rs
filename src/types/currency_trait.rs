use malachite::Rational;

use crate::{Fee, QuoteCurrency};

/// Every unit of account must implement this trait
pub trait Currency:
    Clone
    + Send
    + Sized
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
{
    /// The paired currency.
    /// e.g.: for the BTCUSD market the BTC currency is paired with USD, so the
    /// `PairedCurrency` would be USD
    type PairedCurrency: Currency<PairedCurrency = Self>;

    /// Create a new instance from a `Rational` value
    fn new(val: Rational) -> Self;

    /// Create a new instance from a `f64` value
    fn from_f64(val: f64) -> Self;

    /// Get a reference to the inner `Rational`
    fn inner(&self) -> &Rational;

    /// Create a new currency instance with zero value
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
    fn fee_portion(&self, fee: Fee) -> Self;

    /// Convert this `Currency`'s value into its pair at the conversion `rate`.
    /// E.g:
    /// 1 BTC @ 20_000 USD means that 1 USD = 1 / 20_000 BTC
    fn convert(&self, rate: QuoteCurrency) -> Self::PairedCurrency;

    /// Convert the Currency to a negative value
    fn into_negative(self) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, quote, BaseCurrency, QuoteCurrency};

    #[test]
    fn conversion() {
        assert_eq!(base!(1.0).convert(quote!(20_000.0)), quote!(20_000.0));
        assert_eq!(base!(0.5).convert(quote!(20_000.0)), quote!(10_000.0));
        assert_eq!(base!(0.25).convert(quote!(20_000.0)), quote!(5_000.0));

        assert_eq!(quote!(20_000.0).convert(quote!(20_000.0)), base!(1.0));
        assert_eq!(quote!(10_000.0).convert(quote!(20_000.0)), base!(0.5));
        assert_eq!(quote!(5_000.0).convert(quote!(20_000.0)), base!(0.25));
    }
}
