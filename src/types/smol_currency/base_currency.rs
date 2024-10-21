use std::ops::Neg;

use const_decimal::{Decimal, ParseDecimalError};
use num_traits::{Num, One, Signed, Zero};

use super::{Currency, MarginCurrency, Mon, QuoteCurrency};

/// Representation of a Base currency,
/// e.g in the symbol BTCUSD, the prefix BTC is the `BaseCurrency` and the postfix `USD` is the `QuoteCurrency`.
///
/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `D`: The constant decimal precision
#[derive(
    Debug,
    Clone,
    Default,
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
pub struct BaseCurrency<I, const D: u8>(Decimal<I, D>)
where
    I: Mon<D>;

impl<I, const D: u8> BaseCurrency<I, D>
where
    I: Mon<D>,
{
    /// Create a new instance from an `integer` and a `scale`.
    pub fn new(integer: I, scale: u8) -> Self {
        Self(
            Decimal::try_from_scaled(integer, scale)
                .expect("Can construct `Decimal` from `integer` and `scale`"),
        )
    }
}

/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `D`: The constant decimal precision of the `BaseCurrency`.
impl<I, const D: u8> Currency<I, D> for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    // Generic over the decimal points in paired currency.
    type PairedCurrency = QuoteCurrency<I, D>;

    fn convert_from(units: Self::PairedCurrency, price_per_unit: QuoteCurrency<I, D>) -> Self {
        BaseCurrency(*units.as_ref() / *price_per_unit.as_ref())
    }
}

/// Inverse futures where the `Base` currency is used as margin currency.
///
/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `D`: The constant decimal precision of the `BaseCurrency`.
impl<I, const D: u8> MarginCurrency<I, D> for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    fn pnl(
        entry_price: QuoteCurrency<I, D>,
        exit_price: QuoteCurrency<I, D>,
        quantity: QuoteCurrency<I, D>,
    ) -> BaseCurrency<I, D> {
        if quantity.is_zero() {
            return BaseCurrency::zero();
        }
        BaseCurrency::convert_from(quantity, entry_price)
            - BaseCurrency::convert_from(quantity, exit_price)
    }

    fn price_paid_for_qty(total_cost: Self, quantity: Self::PairedCurrency) -> QuoteCurrency<I, D> {
        if total_cost.is_zero() {
            return QuoteCurrency::zero();
        }
        QuoteCurrency::from(*quantity.as_ref() / *total_cost.as_ref())
    }
}

impl<I, const D: u8> Zero for BaseCurrency<I, D>
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

impl<I, const D: u8> One for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn one() -> Self {
        Self(Decimal::one())
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

impl<I, const D: u8> Num for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    type FromStrRadixErr = ParseDecimalError<I>;

    fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        Ok(Self(Decimal::from_str_radix(str, radix)?))
    }
}

impl<I, const D: u8> Signed for BaseCurrency<I, D>
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

impl<I, const D: u8> std::ops::Mul<Decimal<I, D>> for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Decimal<I, D>) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl<I, const D: u8> std::ops::Div<Decimal<I, D>> for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    type Output = Self;

    #[inline]
    fn div(self, rhs: Decimal<I, D>) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl<I, const D: u8> std::ops::Rem for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    type Output = Self;

    #[inline]
    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

impl<I, const D: u8> std::fmt::Display for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Base", self.0)
    }
}

impl<I, const D: u8> Into<f64> for BaseCurrency<I, D>
where
    I: Mon<D>,
{
    #[inline]
    fn into(self) -> f64 {
        self.0.to_f64()
    }
}
