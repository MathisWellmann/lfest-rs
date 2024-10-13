mod base_currency;
mod convert_decimal;
mod margin_currency_trait;
// mod monies;
mod quote_currency;

pub use base_currency::BaseCurrency;
use const_decimal::ScaledInteger;
pub use margin_currency_trait::MarginCurrencyMarker;
pub use quote_currency::QuoteCurrency;

use super::BasisPointFrac;

/// A trait for monetary values.
pub trait Mon<const D: u8>:
    ScaledInteger<D>
    + Default
    + std::ops::Rem
    + std::ops::Neg
    + num_traits::CheckedNeg
    + std::ops::SubAssign
    + std::hash::Hash
    + std::fmt::Debug
    + num_traits::Signed
{
}

impl<const D: u8> Mon<D> for i32 {}
impl<const D: u8> Mon<D> for i64 {}
impl<const D: u8> Mon<D> for i128 {}

/// A currency must be marked as it can be either a `Base` or `Quote` currency.
///
/// # Generics:
/// - `I` is the numeric type
/// - `DB` is the decimal precision of the `BaseCurrency`.
/// - `DQ` is the decimal precision of the `QuoteCurrency`.
pub trait CurrencyMarker<I, const DB: u8, const DQ: u8>:
    Clone
    + Copy
    + Default
    + std::fmt::Debug
    + std::fmt::Display
    + std::cmp::PartialOrd
    + Eq
    + Ord
    + std::ops::AddAssign
    + std::ops::SubAssign
    + std::ops::Mul<BasisPointFrac>
    + std::ops::Div<I>
    + std::hash::Hash
    + num_traits::Zero
    + num_traits::One
    + num_traits::Signed
    + Into<f64>
where
    I: Mon<DQ> + Mon<DB>,
{
    /// The paired currency in the `Symbol` with generic decimal precision `DP`.
    type PairedCurrency: CurrencyMarker<I, DB, DQ, PairedCurrency = Self>;

    /// Convert from one currency to another at a given price per unit.
    fn convert_from(units: Self::PairedCurrency, price_per_unit: QuoteCurrency<I, DB, DQ>) -> Self;
}

#[cfg(test)]
mod tests {
    use fpdec::{Dec, Decimal};

    use super::*;
    use crate::prelude::*;

    #[test]
    fn convert_base_to_quote() {
        assert_eq!(
            Quote::convert_from(Monies::<_, Base>::new(Dec!(0.5)), Monies::new(Dec!(1000))),
            Monies::<_, Quote>::new(Dec!(500))
        );
    }

    #[test]
    fn convert_quote_to_base() {
        assert_eq!(
            Base::convert_from(Monies::<_, Quote>::new(Dec!(250)), Monies::new(Dec!(1000))),
            Monies::<_, Base>::new(Dec!(0.25))
        );
    }

    #[test]
    fn quote_currency_pnl() {
        assert_eq!(Quote::pnl(quote!(100), quote!(110), base!(5)), quote!(50));
    }

    #[test]
    fn base_currency_pnl() {
        assert_eq!(Base::pnl(quote!(100), quote!(200), quote!(500)), base!(2.5))
    }
}
