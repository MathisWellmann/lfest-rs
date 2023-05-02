use fpdec::Decimal;

use crate::{
    prelude::Leverage,
    types::{Fee, QuoteCurrency},
};

/// Every unit of account must implement this trait
pub trait Currency:
    Copy
    + Send
    + Sized
    + std::fmt::Debug
    + std::fmt::Display
    // Require to do arithmetic with `Self` on the right hand side
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
    + std::ops::Rem<Output = Self>
    // Require to do arithmetic with `Decimal` on the right hand side
    + std::ops::Add<Decimal, Output = Self>
    + std::ops::Sub<Decimal, Output = Self>
    + std::ops::Mul<Decimal, Output = Self>
    + std::ops::Div<Decimal, Output = Self>
    + std::ops::Div<Leverage, Output = Self>
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
    #[must_use]
    fn new(val: Decimal) -> Self;

    /// Return the inner `Decimal`
    fn inner(self) -> Decimal;

    /// Create a new currency instance with zero value
    #[must_use]
    fn new_zero() -> Self;

    /// Check if the value is zero
    fn is_zero(&self) -> bool;

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
    /// TODO: rename for greater clarity
    fn into_negative(self) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn conversion() {
        assert_eq!(base!(1.0).convert(quote!(200.0)), quote!(200.0));
        assert_eq!(base!(0.5).convert(quote!(200.0)), quote!(100.0));
        assert_eq!(base!(0.25).convert(quote!(200.0)), quote!(50.0));

        assert_eq!(quote!(200.0).convert(quote!(200.0)), base!(1.0));
        assert_eq!(quote!(100.0).convert(quote!(200.0)), base!(0.5));
        assert_eq!(quote!(50.0).convert(quote!(200.0)), base!(0.25));
    }
}
