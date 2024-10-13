use std::fmt::Display;

use fpdec::Decimal;
use num_traits::{Num, Signed, Zero};

use super::MarginCurrencyMarker;

mod monies;

pub use monies::{BaseCurrency, Monies, QuoteCurrency};

/// A money like trait which must satisfy a bunch of trait bounds.
pub trait Mon:
    Num
    + Signed
    + Default
    + Clone
    + Copy
    + std::fmt::Debug
    + std::cmp::PartialOrd
    + Ord
    + std::hash::Hash
    + From<u8>
    + From<i32>
    + From<u64>
    + Into<f32>
{
}

impl Mon for Decimal {}

/// A currency must be marked as it can be either a `Base` or `Quote` currency.
pub trait CurrencyMarker<T: Mon>:
    Clone
    + Copy
    + Default
    + std::fmt::Debug
    + std::fmt::Display
    + std::cmp::PartialOrd
    + Eq
    + Ord
    + std::hash::Hash
{
    /// The paired currency in the `Symbol`
    type PairedCurrency: CurrencyMarker<T, PairedCurrency = Self>;

    /// Convert from one currency to another at a given price per unit.
    fn convert_from(
        units: Monies<T, Self::PairedCurrency>,
        price_per_unit: Monies<T, Quote>,
    ) -> Monies<T, Self>;
}

/// The `Base` currency in a market is the prefix in the `Symbol`,
/// e.g BTCUSD means BTC is the base currency, quoted in USD.
#[derive(Debug, Default, Clone, Copy, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub struct Base;

impl Display for Base {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Base")
    }
}

/// The `Quote` currency in the market is the postfix in the `Symbol`,
/// e.g BTCUSD means BTC is the base currency, quoted in USD.
#[derive(Debug, Default, Clone, Copy, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub struct Quote;

impl Display for Quote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Quote")
    }
}

impl<T: Mon> CurrencyMarker<T> for Base {
    type PairedCurrency = Quote;

    fn convert_from(
        units: Monies<T, Self::PairedCurrency>,
        price_per_unit: Monies<T, Quote>,
    ) -> Monies<T, Self> {
        Monies::new(*units.as_ref() / *price_per_unit.as_ref())
    }
}

impl<T: Mon> CurrencyMarker<T> for Quote {
    type PairedCurrency = Base;

    fn convert_from(
        units: Monies<T, Self::PairedCurrency>,
        price_per_unit: Monies<T, Quote>,
    ) -> Monies<T, Self> {
        Monies::new(*units.as_ref() * *price_per_unit.as_ref())
    }
}

/// Linear futures where the `Quote` currency is used as margin currency.
impl<T> MarginCurrencyMarker<T> for Quote
where
    T: Mon,
{
    /// This represents a linear futures contract pnl calculation
    fn pnl(
        entry_price: Monies<T, Quote>,
        exit_price: Monies<T, Quote>,
        quantity: Monies<T, Base>,
    ) -> Monies<T, Quote> {
        if quantity.is_zero() {
            return Monies::zero();
        }
        Quote::convert_from(quantity, exit_price) - Quote::convert_from(quantity, entry_price)
    }

    fn price_paid_for_qty(
        total_cost: Monies<T, Self>,
        quantity: Monies<T, Self::PairedCurrency>,
    ) -> Monies<T, Quote> {
        if quantity.is_zero() {
            return Monies::zero();
        }
        Monies::new(*total_cost.as_ref() / *quantity.as_ref())
    }
}

/// Inverse futures where the `Base` currency is used as margin currency.
impl<T> MarginCurrencyMarker<T> for Base
where
    T: Mon,
{
    fn pnl(
        entry_price: Monies<T, Quote>,
        exit_price: Monies<T, Quote>,
        quantity: Monies<T, Quote>,
    ) -> Monies<T, Base> {
        if quantity.is_zero() {
            return Monies::zero();
        }
        Base::convert_from(quantity, entry_price) - Base::convert_from(quantity, exit_price)
    }

    fn price_paid_for_qty(
        total_cost: Monies<T, Self>,
        quantity: Monies<T, Self::PairedCurrency>,
    ) -> Monies<T, Quote> {
        if total_cost.is_zero() {
            return Monies::zero();
        }
        Monies::new(*quantity.as_ref() / *total_cost.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use fpdec::{Dec, Decimal};

    use super::*;
    use crate::prelude::*;

    #[test]
    fn size_of_monies() {
        // assert_eq!(std::mem::size_of::<Monies<i32, Base>>(), 4);
        assert_eq!(std::mem::size_of::<Monies<Decimal, Base>>(), 32);
    }

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
