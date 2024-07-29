use std::ops::Mul;

use derive_more::Display;
use fpdec::Decimal;

use super::{Currency, QuoteCurrency};

/// Allows the quick construction of `Fee`
#[macro_export]
macro_rules! fee {
    ( $a:literal) => {{
        use $crate::prelude::fpdec::Decimal;
        $crate::prelude::Fee::new($crate::prelude::fpdec::Dec!($a))
    }};
}

/// Fee as a fraction
#[derive(Default, Debug, Clone, Copy, PartialEq, Display)]
pub struct Fee(Decimal);

impl Fee {
    /// Create a new instance from a `Decimal` value
    #[inline(always)]
    pub const fn new(val: Decimal) -> Self {
        Self(val)
    }
}

impl<C> Mul<C> for Fee
where
    C: Currency,
{
    type Output = QuoteCurrency;

    fn mul(self, rhs: C) -> Self::Output {
        QuoteCurrency::new(self.0 * rhs.as_ref())
    }
}

impl AsRef<Decimal> for Fee {
    fn as_ref(&self) -> &Decimal {
        &self.0
    }
}

/// The two types of fees in the maker-taker model.
#[derive(Debug, Clone)]
pub enum FeeType {
    /// The fee limit orders pay.
    Maker(Fee),
    /// The fee market orders pay.
    Taker(Fee),
}
