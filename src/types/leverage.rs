use std::convert::TryFrom;

use derive_more::Display;
use fpdec::Decimal;

/// Allows the quick construction of `Leverage`
#[macro_export]
macro_rules! leverage {
    ( $a:expr ) => {{
        Leverage::from_f64($a)
    }};
}

/// Leverage
/// TODO: Change this to u8 type, as no fractional leverage should be possible
#[derive(Default, Debug, Clone, Copy, PartialEq, Display)]
pub struct Leverage(Decimal);

impl Leverage {
    #[inline]
    pub(crate) fn from_f64(val: f64) -> Self {
        Self(Decimal::try_from(val).expect("Unable to create Decimal from f64"))
    }

    #[inline(always)]
    pub(crate) fn inner(&self) -> Decimal {
        self.0
    }
}
