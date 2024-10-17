use std::ops::Neg;

use const_decimal::Decimal;
use num_traits::{Num, One, Signed, Zero};

use super::{CurrencyMarker, MarginCurrencyMarker, Mon, QuoteCurrency};
use crate::prelude::BasisPointFrac;

/// Representation of a Base currency,
/// e.g in the symbol BTCUSD, the prefix BTC is the `BaseCurrency` and the postfix `USD` is the `QuoteCurrency`.
///
/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `DB`: The constant decimal precision of the `BaseCurrency` (Self).
/// - `DQ`: The constant decimal precision of the `QuoteCurrency`.
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
pub struct BaseCurrency<I, const DB: u8, const DQ: u8>(Decimal<I, DB>)
where
    I: Mon<DQ> + Mon<DB>;

impl<I, const DB: u8, const DQ: u8> BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    /// Create a new instance from an `integer` and a `scale`.
    pub fn new(integer: I, scale: u8) -> Self {
        Self(Decimal::try_from_scaled(integer, scale).expect("Make sure the inputs are correct."))
    }
}

/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `DB`: The constant decimal precision of the `BaseCurrency`.
/// - `DQ`: The constant decimal precision of the `QuoteCurrency`.
impl<I, const DB: u8, const DQ: u8> CurrencyMarker<I, DB, DQ> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    // Generic over the decimal points in paired currency.
    type PairedCurrency = QuoteCurrency<I, DB, DQ>;

    fn convert_from(units: Self::PairedCurrency, price_per_unit: QuoteCurrency<I, DB, DQ>) -> Self {
        let scaled =
            Decimal::<I, DB>::try_from_scaled((*units.as_ref() / *price_per_unit.as_ref()).0, DQ)
                .expect("can scale");
        BaseCurrency(scaled)
    }
}

/// Inverse futures where the `Base` currency is used as margin currency.
///
/// # Generics:
/// - `I`: The numeric data type of `Decimal`.
/// - `DB`: The constant decimal precision of the `BaseCurrency`.
/// - `DQ`: The constant decimal precision of the `QuoteCurrency`.
impl<I, const DB: u8, const DQ: u8> MarginCurrencyMarker<I, DB, DQ> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    fn pnl(
        entry_price: QuoteCurrency<I, DB, DQ>,
        exit_price: QuoteCurrency<I, DB, DQ>,
        quantity: QuoteCurrency<I, DB, DQ>,
    ) -> BaseCurrency<I, DB, DQ> {
        if quantity.is_zero() {
            return BaseCurrency::zero();
        }
        BaseCurrency::convert_from(quantity, entry_price)
            - BaseCurrency::convert_from(quantity, exit_price)
    }

    fn price_paid_for_qty(
        total_cost: Self,
        quantity: Self::PairedCurrency,
    ) -> QuoteCurrency<I, DB, DQ> {
        if total_cost.is_zero() {
            return QuoteCurrency::zero();
        }
        // QuoteCurrency::new(quantity / total_cost)
        todo!()
    }
}

impl<I, const DB: u8, const DQ: u8> Zero for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    fn zero() -> Self {
        Self(Decimal::<I, DB>::try_from_scaled(I::zero(), 0).unwrap())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl<I, const DB: u8, const DQ: u8> One for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    fn one() -> Self {
        Self(Decimal::<I, DB>::try_from_scaled(I::one(), 0).unwrap())
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

impl<I, const DB: u8, const DQ: u8> Num for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    type FromStrRadixErr = &'static str;

    fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        panic!("Not needed")
    }
}

impl<I, const DB: u8, const DQ: u8> Signed for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB> + Signed,
{
    #[inline]
    fn abs(&self) -> Self {
        Self(Decimal::try_from_scaled(self.0 .0.abs(), DB).unwrap())
    }

    #[inline]
    fn abs_sub(&self, other: &Self) -> Self {
        todo!("not needed")
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

impl<I, const DB: u8, const DQ: u8> std::ops::Mul<Decimal<I, DB>> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    type Output = Self;

    fn mul(self, rhs: Decimal<I, DB>) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl<I, const DB: u8, const DQ: u8> std::ops::Mul<BasisPointFrac> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    type Output = Self;

    fn mul(self, rhs: BasisPointFrac) -> Self::Output {
        // Self(self.0 * *rhs.as_ref())
        todo!()
    }
}

impl<I, const DB: u8, const DQ: u8> std::ops::Div<Decimal<I, DB>> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    type Output = Self;

    fn div(self, rhs: Decimal<I, DB>) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl<I, const DB: u8, const DQ: u8> std::ops::Div<I> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    type Output = Self;

    fn div(self, rhs: I) -> Self::Output {
        Self(self.0 / Decimal::try_from_scaled(rhs, DB).unwrap())
    }
}

impl<I, const DB: u8, const DQ: u8> std::ops::Rem for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

impl<I, const DB: u8, const DQ: u8> std::fmt::Display for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Quote", self.0)
    }
}

impl<I, const DB: u8, const DQ: u8> Into<f64> for BaseCurrency<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
{
    fn into(self) -> f64 {
        self.0.to_f64()
    }
}
