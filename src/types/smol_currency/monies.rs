use std::ops::Neg;

use const_decimal::Decimal;
use num_traits::{Num, One, Signed, Zero};

use super::Mon;
use crate::prelude::BasisPointFrac;

/// A currency must be marked as it can be either a `Base` or `Quote` currency.
trait CurrencyMarker:
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
    type PairedCurrency: CurrencyMarker<PairedCurrency = Self>;

    // /// Convert from one currency to another at a given price per unit.
    // fn convert_from<const DQ: u8>(
    //     units: Monies<I, DP, Self::PairedCurrency>,
    //     price_per_unit: Monies<I, DQ, Quote>,
    // ) -> Monies<I, D_SELF, Self>;
}

/// A generic monetary data type with a marker for being either denominated in `Base` or `Quote` currency.
#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Monies<I, const D: u8, BaseOrQuote>
where
    BaseOrQuote: CurrencyMarker,
{
    value: I,
    _marker: std::marker::PhantomData<BaseOrQuote>,
}

impl<I, const D: u8, BaseOrQuote> From<Decimal<I, D>> for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn from(value: Decimal<I, D>) -> Self {
        Self {
            value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> AsRef<Decimal<I, D>> for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn as_ref(&self) -> &Decimal<I, D> {
        &self.value
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {:?}", self.value, self._marker)
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::Add for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type Output = Self;

    fn add(self, rhs: Monies<I, D, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value + rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::AddAssign for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn add_assign(&mut self, rhs: Monies<I, D, BaseOrQuote>) {
        *self = *self + rhs
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::Sub for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type Output = Self;

    fn sub(self, rhs: Monies<I, D, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value - rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::SubAssign for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn sub_assign(&mut self, rhs: Monies<I, D, BaseOrQuote>) {
        *self = *self - rhs;
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::Mul for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type Output = Self;

    fn mul(self, rhs: Monies<I, D, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value * rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::Div for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type Output = Self;

    fn div(self, rhs: Monies<I, D, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value / rhs.value,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> std::ops::Rem for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type Output = Self;

    fn rem(self, rhs: Monies<I, D, BaseOrQuote>) -> Self::Output {
        Self {
            value: self.value.rem(rhs.value),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8> Monies<I, D, Quote>
where
    I: Mon<D>,
{
    /// Compute the liquidation price of a long position given a maintenance margin requirement fraction
    pub(crate) fn liquidation_price_long(&self, maint_margin_req: BasisPointFrac) -> Self {
        Self {
            value: self.value * (Decimal::one() - maint_margin_req),
            _marker: std::marker::PhantomData,
        }
    }

    /// Compute the liquidation price of a short position given a maintenance margin requirement fraction
    pub(crate) fn liquidation_price_short(&self, maint_margin_req: BasisPointFrac) -> Self {
        Self {
            value: self.value * (Decimal::one() + maint_margin_req),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> Zero for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn zero() -> Self {
        Self {
            value: Decimal::zero(),
            _marker: std::marker::PhantomData,
        }
    }

    fn is_zero(&self) -> bool {
        self.value.is_zero()
    }
}

impl<I, const D: u8, BaseOrQuote> One for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    fn one() -> Self {
        Self {
            value: Decimal::one(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> Num for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type FromStrRadixErr = &'static str;

    fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        Ok(Self {
            value: Decimal::from_str_radix(str, radix).map_err(|_| "Unsupported")?,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<I, const D: u8, BaseOrQuote> Neg for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
{
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            value: self.value.neg(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> Signed for Monies<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker,
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

/// The `Base` currency in a market is the prefix in the `Symbol`,
/// e.g BTCUSD means BTC is the base currency, quoted in USD.
#[derive(Debug, Default, Clone, Copy, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub struct Base;

impl std::fmt::Display for Base {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Base")
    }
}

/// The `Quote` currency in the market is the postfix in the `Symbol`,
/// e.g BTCUSD means BTC is the base currency, quoted in USD.
#[derive(Debug, Default, Clone, Copy, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub struct Quote;

impl std::fmt::Display for Quote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Quote")
    }
}

impl CurrencyMarker for Base {
    type PairedCurrency = Quote;

    // fn convert_from<const DQ: u8>(
    //     units: Monies<I, Self::DP, Self::PairedCurrency>,
    //     price_per_unit: Monies<I, DB, Quote>,
    // ) -> Monies<I, DQ, Self> {
    //     Monies::new(*units.as_ref() / *price_per_unit.as_ref())
    // }
}

impl CurrencyMarker for Quote {
    type PairedCurrency = Base;

    // fn convert_from(
    //     units: Monies<T, Self::PairedCurrency>,
    //     price_per_unit: Monies<T, Quote>,
    // ) -> Monies<T, Self> {
    //     Monies::new(*units.as_ref() * *price_per_unit.as_ref())
    // }
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
