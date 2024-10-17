mod base_currency;
mod margin_currency_trait;
mod quote_currency;

pub use base_currency::BaseCurrency;
use const_decimal::{Decimal, ScaledInteger};
pub use margin_currency_trait::MarginCurrencyMarker;
pub use quote_currency::QuoteCurrency;

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
/// - `D` is the decimal precision.
pub trait CurrencyMarker<I, const D: u8>:
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
    + std::ops::Mul<Decimal<I, D>, Output = Self>
    + std::hash::Hash
    + num_traits::Zero
    + num_traits::One
    + num_traits::Signed
    + Into<f64>
where
    I: Mon<D>,
{
    /// The paired currency in the `Symbol` with generic decimal precision `DP`.
    type PairedCurrency: CurrencyMarker<I, D, PairedCurrency = Self>;

    /// Convert from one currency to another at a given price per unit.
    fn convert_from(units: Self::PairedCurrency, price_per_unit: QuoteCurrency<I, D>) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_base_to_quote() {
        // 0.5 BTC @ 100 USD = 50 USD
        assert_eq!(
            QuoteCurrency::convert_from(
                BaseCurrency::<i32, 4>::new(5, 1),
                QuoteCurrency::new(100, 0)
            ),
            QuoteCurrency::new(50, 0)
        );
    }

    #[test]
    fn convert_quote_to_base() {
        assert_eq!(
            BaseCurrency::convert_from(
                QuoteCurrency::<i32, 4>::new(250, 0),
                QuoteCurrency::new(1000, 0)
            ),
            BaseCurrency::new(25, 2)
        );
    }

    #[test]
    fn quote_currency_pnl() {
        assert_eq!(
            QuoteCurrency::pnl(
                QuoteCurrency::<i64, 4>::new(100, 0),
                QuoteCurrency::new(110, 0),
                BaseCurrency::new(5, 0),
            ),
            QuoteCurrency::new(50, 0)
        );
    }

    #[test]
    fn base_currency_pnl() {
        assert_eq!(
            BaseCurrency::pnl(
                QuoteCurrency::<i32, 4>::new(100, 0),
                QuoteCurrency::new(200, 0),
                QuoteCurrency::new(500, 0),
            ),
            BaseCurrency::new(25, 1)
        )
    }
}
