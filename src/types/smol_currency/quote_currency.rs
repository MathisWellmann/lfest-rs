use std::ops::Neg;

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
    /// Create a new instance from an `integer` and a `scale`.
    pub fn new(integer: I, scale: u8) -> Self {
        Self(Decimal::try_from_scaled(integer, scale).expect("Make sure the inputs are correct."))
    }

    #[inline]
    pub(crate) fn liquidation_price_long(&self, maint_margin_req: Decimal<I, D>) -> Self {
        Self(self.0 * (Decimal::one() - maint_margin_req))
    }

    #[inline]
    pub(crate) fn liquidation_price_short(&self, maint_margin_req: Decimal<I, D>) -> Self {
        Self(self.0 * (Decimal::one() + maint_margin_req))
    }

    pub(crate) fn new_weighted_price(
        price_0: Self,
        weight_0: Decimal<I, D>,
        price_1: Self,
        weight_1: Decimal<I, D>,
    ) -> Self {
        let total_weight = weight_0 + weight_1;
        (price_0 * weight_0 + price_1 * weight_1) / total_weight
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
    fn pnl(
        entry_price: QuoteCurrency<I, D>,
        exit_price: QuoteCurrency<I, D>,
        quantity: BaseCurrency<I, D>,
    ) -> QuoteCurrency<I, D> {
        if quantity.is_zero() {
            return QuoteCurrency::zero();
        }
        QuoteCurrency::convert_from(quantity, exit_price)
            - QuoteCurrency::convert_from(quantity, entry_price)
    }

    fn price_paid_for_qty(total_cost: Self, quantity: Decimal<I, D>) -> QuoteCurrency<I, D> {
        if quantity.is_zero() {
            return QuoteCurrency::zero();
        }

        QuoteCurrency(*total_cost.as_ref() / quantity)
    }
}

impl<I, const D: u8> Zero for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn zero() -> Self {
        Self(Decimal::<I, D>::try_from_scaled(I::zero(), 0).unwrap())
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

impl<I, const D: u8> Into<f64> for QuoteCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn into(self) -> f64 {
        self.0.to_f64()
    }
}

#[cfg(test)]
mod test {
    use std::ops::{Div, Rem};

    use super::*;

    #[test]
    fn base_currency() {
        let v = QuoteCurrency::<i64, 5>::new(100, 0);
        assert!(v.is_positive());
        assert!(!v.is_negative());
        let v = QuoteCurrency::<i64, 5>::new(-100, 0);
        assert!(!v.is_positive());
        assert!(v.is_negative());
        let v = QuoteCurrency::<i64, 5>::new(0, 0);
        assert!(v.is_zero());
        assert!(!v.is_one());
        let v = QuoteCurrency::<i64, 5>::new(1, 0);
        assert!(v.is_one());
        assert_eq!(Into::<f64>::into(v), 1_f64);
        let v = QuoteCurrency::<i64, 5>::new(8, 0);
        assert_eq!(v.rem(QuoteCurrency::new(5, 0)), QuoteCurrency::new(3, 0));
        assert_eq!(v.div(QuoteCurrency::new(2, 0)), QuoteCurrency::new(4, 0));
    }

    #[test]
    fn quote_currency_price_paid_for_qty() {
        assert_eq!(
            QuoteCurrency::price_paid_for_qty(
                QuoteCurrency::<i64, 5>::new(1000, 0),
                Decimal::try_from_scaled(5, 0).unwrap()
            ),
            QuoteCurrency::new(200, 0)
        );
    }
}
