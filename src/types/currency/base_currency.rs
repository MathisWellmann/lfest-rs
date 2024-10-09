use std::ops::Rem;

use derive_more::{Add, AddAssign, Div, From, Into, Mul, Sub, SubAssign};
use fpdec::{Dec, Decimal, Quantize};

use super::MarginCurrency;
use crate::{
    prelude::Leverage,
    quote,
    types::{Currency, Fee, QuoteCurrency},
};

/// Allows the quick construction of `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:literal ) => {{
        use $crate::prelude::{fpdec::Decimal, Currency};
        $crate::prelude::BaseCurrency::new($crate::prelude::fpdec::Dec!($a))
    }};
}

/// The markets BASE currency, e.g.: BTCUSD -> BTC is the BASE currency
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Add,
    Sub,
    Mul,
    Div,
    AddAssign,
    SubAssign,
    Into,
    From,
    Hash,
    Serialize,
    Deserialize,
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
    fn convert(&self, rate: QuoteCurrency) -> Self::PairedCurrency {
        QuoteCurrency::new(self.0 * rate.as_ref())
    }

    #[inline(always)]
    fn into_negative(self) -> Self {
        Self(-self.0)
    }

    fn quantize(self, val: Self) -> Self {
        Self(self.0.quantize(*val.as_ref()))
    }
}

impl MarginCurrency for BaseCurrency {
    /// This represents the pnl calculation for inverse futures contracts
    fn pnl<S: Currency>(
        entry_price: QuoteCurrency,
        exit_price: QuoteCurrency,
        quantity: S,
    ) -> S::PairedCurrency {
        if quantity.is_zero() {
            return S::PairedCurrency::new_zero();
        }
        quantity.convert(entry_price) - quantity.convert(exit_price)
    }

    // inverse futures.
    fn price_paid_for_qty(&self, quantity: <Self as Currency>::PairedCurrency) -> QuoteCurrency {
        if self.0 == Dec!(0) {
            return quote!(0);
        }
        QuoteCurrency::from(quantity.as_ref() / self.0)
    }
}

impl AsRef<Decimal> for BaseCurrency {
    fn as_ref(&self) -> &Decimal {
        &self.0
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

impl Div<Leverage> for BaseCurrency {
    type Output = Self;

    fn div(self, rhs: Leverage) -> Self::Output {
        Self(self.0 / *rhs.as_ref())
    }
}

impl Mul<Fee> for BaseCurrency {
    type Output = Self;

    fn mul(self, rhs: Fee) -> Self::Output {
        Self(self.0 * rhs.as_ref())
    }
}

impl std::fmt::Display for BaseCurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} BASE", self.0)
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
        assert_eq!(
            BaseCurrency::pnl(quote!(100.0), quote!(125.0), quote!(1000.0)),
            base!(2.0)
        );
        assert_eq!(
            BaseCurrency::pnl(quote!(100.0), quote!(125.0), quote!(-1000.0)),
            base!(-2.0)
        );
        assert_eq!(
            BaseCurrency::pnl(quote!(100.0), quote!(80.0), quote!(1000.0)),
            base!(-2.5)
        );
        assert_eq!(
            BaseCurrency::pnl(quote!(100.0), quote!(80.0), quote!(-1000.0)),
            base!(2.5)
        );
    }

    #[test]
    fn parse_base_currency() {
        let v = base!(0.1);
        let ser = ron::ser::to_string_pretty(&v, ron::ser::PrettyConfig::new().struct_names(true))
            .unwrap();
        let s = r#"BaseCurrency("0.1")"#;
        assert_eq!(ser, s);
        assert_eq!(ron::from_str::<BaseCurrency>(s).unwrap(), v);
    }
}
