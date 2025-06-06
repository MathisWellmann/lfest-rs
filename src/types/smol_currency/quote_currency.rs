use std::{iter::Sum, ops::Neg};

use const_decimal::{Decimal, ParseDecimalError};
use num_traits::{Num, One, Signed, Zero};

use super::{BaseCurrency, Currency, MarginCurrency, Mon};

/// Representation of a Quote currency,
/// e.g in the symbol BTCUSD, the prefix BTC is the `BaseCurrency` and the postfix `USD` is the `QuoteCurrency`.
///
/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `D`: The constant decimal precision.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    std::hash::Hash,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::Div,
    derive_more::Neg,
    derive_more::From,
    derive_more::AsRef,
)]
#[mul(forward)]
#[div(forward)]
#[repr(transparent)]
pub struct QuoteCurrency<I, const D: u8>(Decimal<I, D>)
where
    I: Mon<D>;

impl<I, const D: u8> QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    // TODO: return `Result`
    /// Create a new instance from an `integer` and a `scale`.
    pub fn new(integer: I, scale: u8) -> Self {
        assert2::debug_assert!(scale <= D);
        Self(Decimal::try_from_scaled(integer, scale).expect("Make sure the inputs are correct."))
    }

    #[inline]
    pub(crate) fn liquidation_price_long(&self, maint_margin_req: Decimal<I, D>) -> Self {
        assert2::debug_assert!(maint_margin_req <= Decimal::ONE);
        Self(self.0 * (Decimal::one() - maint_margin_req))
    }

    #[inline]
    pub(crate) fn liquidation_price_short(&self, maint_margin_req: Decimal<I, D>) -> Self {
        assert2::debug_assert!(maint_margin_req <= Decimal::ONE);
        Self(self.0 * (Decimal::one() + maint_margin_req))
    }

    pub(crate) fn new_weighted_price(
        price_0: Self,
        weight_0: Decimal<I, D>,
        price_1: Self,
        weight_1: Decimal<I, D>,
    ) -> Self {
        assert2::debug_assert!(price_0 >= Zero::zero());
        assert2::debug_assert!(weight_0 > Zero::zero());
        assert2::debug_assert!(price_1 >= Zero::zero());
        assert2::debug_assert!(weight_1 > Zero::zero());
        let total_weight = weight_0 + weight_1;
        let weighted_price = (price_0 * weight_0 + price_1 * weight_1) / total_weight;
        assert2::debug_assert!(weighted_price > Zero::zero());
        weighted_price
    }

    /// Round a number to a multiple of a given `quantum` toward zero.
    /// general ref: <https://en.wikipedia.org/wiki/Quantization_(signal_processing)>
    ///
    /// By default, rust is rounding towards zero and so does this method.
    ///
    /// # Example:
    /// ```rust
    /// use lfest::prelude::QuoteCurrency;
    /// // 11.65
    /// let d = QuoteCurrency::<i64, 5>::new(1165, 2);
    /// // Allow only increments of 0.5
    /// let quantum = QuoteCurrency::<i64, 5>::new(5, 1);
    /// let q = d.quantize_round_to_zero(quantum);
    /// // 11.5 rounded down to the nearest `quantum`.
    /// assert_eq!(q, QuoteCurrency::new(115, 1));
    /// ```
    #[inline]
    #[must_use]
    pub fn quantize_round_to_zero(&self, quantum: Self) -> Self {
        Self(self.0.quantize_round_to_zero(*quantum.as_ref()))
    }
}

/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `D`: The constant decimal precision of the `QuoteCurrency`.
impl<I, const D: u8> Currency<I, D> for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    type PairedCurrency = BaseCurrency<I, D>;

    fn convert_from(units: Self::PairedCurrency, price_per_unit: QuoteCurrency<I, D>) -> Self {
        assert2::debug_assert!(price_per_unit > Zero::zero());
        QuoteCurrency(*units.as_ref() * *price_per_unit.as_ref())
    }
}

/// Linear futures where the `Quote` currency is used as margin currency.
///
/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `D`: The constant decimal precision.
impl<I, const D: u8> MarginCurrency<I, D> for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    /// This represents a linear futures contract pnl calculation
    #[inline]
    fn pnl(
        entry_price: QuoteCurrency<I, D>,
        exit_price: QuoteCurrency<I, D>,
        quantity: BaseCurrency<I, D>,
    ) -> QuoteCurrency<I, D> {
        assert2::debug_assert!(entry_price > Zero::zero());
        assert2::debug_assert!(exit_price > Zero::zero());
        QuoteCurrency::convert_from(quantity, exit_price)
            - QuoteCurrency::convert_from(quantity, entry_price)
    }
}

impl<I, const D: u8> Zero for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn zero() -> Self {
        Self(Decimal::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl<I, const D: u8> One for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn one() -> Self {
        Self(Decimal::<I, D>::try_from_scaled(I::one(), 0).unwrap())
    }

    #[inline]
    fn set_one(&mut self) {
        *self = One::one();
    }

    #[inline]
    fn is_one(&self) -> bool
    where
        Self: PartialEq,
    {
        *self == Self::one()
    }
}

impl<I, const D: u8> Num for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    type FromStrRadixErr = ParseDecimalError<I>;

    fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        Ok(QuoteCurrency(Decimal::from_str_radix(str, radix)?))
    }
}

impl<I, const D: u8> Signed for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    #[inline]
    fn abs_sub(&self, other: &Self) -> Self {
        Self(self.0.abs_sub(&other.0))
    }

    #[inline]
    fn signum(&self) -> Self {
        use std::cmp::Ordering::*;
        match self.0.cmp(&Decimal::zero()) {
            Less => Self(Decimal::one().neg()),
            Equal => Self(Decimal::zero()),
            Greater => Self(Decimal::one()),
        }
    }

    #[inline]
    fn is_positive(&self) -> bool {
        self.0 > Decimal::zero()
    }

    #[inline]
    fn is_negative(&self) -> bool {
        self.0 < Decimal::zero()
    }
}

impl<I, const D: u8> std::ops::Mul<Decimal<I, D>> for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Decimal<I, D>) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl<I, const D: u8> std::ops::Div<Decimal<I, D>> for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    type Output = Self;

    #[inline]
    fn div(self, rhs: Decimal<I, D>) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl<I, const D: u8> std::ops::Rem for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    type Output = Self;

    #[inline]
    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0.rem(rhs.0))
    }
}

impl<I, const D: u8> std::fmt::Display for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Quote", self.0)
    }
}

impl<I, const D: u8> From<QuoteCurrency<I, D>> for f64
where
    I: Mon<D>,
{
    #[inline]
    fn from(val: QuoteCurrency<I, D>) -> Self {
        val.0.to_f64()
    }
}

impl<I, const D: u8> Sum for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    fn sum<T: Iterator<Item = Self>>(iter: T) -> Self {
        let mut out = Self::zero();
        iter.for_each(|v| out += v);
        out
    }
}

#[cfg(test)]
mod test {
    use std::ops::{Div, Rem};

    use super::*;

    #[test]
    fn quote_currency() {
        let v = QuoteCurrency::<i64, 5>::new(100, 0);
        assert_eq!(v.abs_sub(&QuoteCurrency::new(105, 0)), Zero::zero());
        assert_eq!(
            v.abs_sub(&QuoteCurrency::new(95, 0)),
            QuoteCurrency::new(5, 0)
        );
        assert!(v.is_positive());
        assert!(!v.is_negative());
        let v = QuoteCurrency::<i64, 5>::new(-100, 0);
        assert!(!v.is_positive());
        assert!(v.is_negative());
        assert_eq!(v.abs(), QuoteCurrency::new(100, 0));
        let v = QuoteCurrency::<i64, 5>::new(0, 0);
        assert!(v.is_zero());
        assert!(!v.is_one());
        let v = QuoteCurrency::<i64, 5>::new(1, 0);
        assert!(v.is_one());
        assert_eq!(Into::<f64>::into(v), 1_f64);
        let v = QuoteCurrency::<i64, 5>::new(8, 0);
        assert_eq!(v.rem(QuoteCurrency::new(5, 0)), QuoteCurrency::new(3, 0));
        assert_eq!(v % QuoteCurrency::new(5, 0), QuoteCurrency::new(3, 0));
        assert_eq!(v.div(QuoteCurrency::new(2, 0)), QuoteCurrency::new(4, 0));
        assert_eq!(v / QuoteCurrency::new(2, 0), QuoteCurrency::new(4, 0));

        let mut result = QuoteCurrency::from_str_radix("27", 10).unwrap();
        assert_eq!(result, QuoteCurrency::<i64, 5>::new(27, 0));
        result.set_one();
        assert_eq!(result, QuoteCurrency::one());
        assert_eq!(QuoteCurrency::zero(), QuoteCurrency::<i64, 5>::new(0, 0));
    }
}
