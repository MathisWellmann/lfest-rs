use std::ops::Neg;

use fpdec::Decimal;
use num_traits::{Num, One, Signed, Zero};

use super::{Base, CurrencyMarker, Mon, Quote};

/// type alias for easy construction of `Base` currency with `Decimal` data type.
pub type QuoteCurrency = Monies<Decimal, Quote>;

/// type alias for easy construction of `Quote` currency with `Decimal` data type.
pub type BaseCurrency = Monies<Decimal, Base>;

/// Allows the quick construction of `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:literal ) => {{
        use $crate::prelude::fpdec::Decimal;
        $crate::prelude::BaseCurrency::new($crate::prelude::fpdec::Dec!($a))
    }};
}

/// Allows the quick construction of `QuoteCurrency`
#[macro_export]
macro_rules! quote {
    ( $a:literal ) => {{
        use $crate::prelude::fpdec::Decimal;
        $crate::prelude::QuoteCurrency::new($crate::prelude::fpdec::Dec!($a))
    }};
}

/// A generic monetary data type with a marker for being either denominated in `Base` or `Quote` currency.
#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    value: T,
    _marker: std::marker::PhantomData<BaseOrQuote>,
}

impl<T, BaseOrQuote> From<T> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn from(value: T) -> Self {
        Self {
            value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> std::fmt::Display for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {:?}", self.value, self._marker)
    }
}

impl<T, BaseOrQuote> AsRef<T> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T, BaseOrQuote> std::ops::Add<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn add(self, rhs: Monies<T, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value + rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> std::ops::AddAssign<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn add_assign(&mut self, rhs: Monies<T, BaseOrQuote>) {
        *self = *self + rhs
    }
}

impl<T, BaseOrQuote> std::ops::Sub<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn sub(self, rhs: Monies<T, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value - rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> std::ops::SubAssign<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn sub_assign(&mut self, rhs: Monies<T, BaseOrQuote>) {
        *self = *self - rhs;
    }
}

impl<T, BaseOrQuote> std::ops::Mul<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn mul(self, rhs: Monies<T, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value * rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> std::ops::Mul<T> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn mul(self, rhs: T) -> Self::Output {
        Self {
            value: self.value * rhs,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> std::ops::Div<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn div(self, rhs: Monies<T, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value / rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> std::ops::Rem<Monies<T, BaseOrQuote>> for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn rem(self, rhs: Monies<T, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value.rem(rhs.value),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    /// Create a new instance from a value `T`
    pub fn new(value: T) -> Self {
        Self {
            value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Monies<T, Quote>
where
    T: Mon,
{
    /// Compute the liquidation price given a maintenance margin requirement fraction
    pub(crate) fn liquidation_price(&self, maint_margin_req: T) -> Self {
        Self {
            value: self.value * (T::one() - maint_margin_req),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> Zero for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn zero() -> Self {
        Self {
            value: T::zero(),
            _marker: std::marker::PhantomData,
        }
    }

    fn is_zero(&self) -> bool {
        self.value.is_zero()
    }
}

impl<T, BaseOrQuote> One for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn one() -> Self {
        Self {
            value: T::one(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> Num for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type FromStrRadixErr = &'static str;

    fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        Ok(Self {
            value: T::from_str_radix(str, radix).map_err(|_| "Unsupported")?,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<T, BaseOrQuote> Neg for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            value: self.value.neg(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, BaseOrQuote> Signed for Monies<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn abs(&self) -> Self {
        Self {
            value: self.value.abs(),
            _marker: std::marker::PhantomData,
        }
    }

    fn abs_sub(&self, other: &Self) -> Self {
        Self {
            value: self.value.abs_sub(&other.value),
            _marker: std::marker::PhantomData,
        }
    }

    fn signum(&self) -> Self {
        Self {
            value: self.value.signum(),
            _marker: std::marker::PhantomData,
        }
    }

    fn is_positive(&self) -> bool {
        self.value.is_positive()
    }

    fn is_negative(&self) -> bool {
        self.value.is_negative()
    }
}

#[cfg(test)]
mod tests {
    use fpdec::{Dec, Decimal};

    use super::*;
    use crate::prelude::Base;

    #[test]
    fn add_monies() {
        assert_eq!(
            Monies::<_, Base>::new(Dec!(0.5)) + Monies::<_, Base>::new(Dec!(0.5)),
            Monies::<_, Base>::new(Dec!(1))
        );
    }
}
