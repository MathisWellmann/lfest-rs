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
        todo!()
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

    fn price_paid_for_qty(total_cost: Self, quantity: Self::PairedCurrency) -> QuoteCurrency<I, D> {
        if quantity.is_zero() {
            return QuoteCurrency::zero();
        }

        QuoteCurrency(*total_cost.as_ref() / *quantity.as_ref())
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
