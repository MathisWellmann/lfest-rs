use std::convert::TryFrom;

use derive_more::Display;
use fpdec::Decimal;

/// Allows the quick construction of `Fee`
#[macro_export]
macro_rules! fee {
    ( $a:expr ) => {{
        Fee::from_f64($a)
    }};
}

/// Fee as a fraction
#[derive(Default, Debug, Clone, Copy, PartialEq, Display)]
pub struct Fee(Decimal);

impl Fee {
    /// Create a new `Fee` instance from an `f64` value
    #[inline]
    pub fn from_f64(val: f64) -> Self {
        Self(Decimal::try_from(val).expect("Unable to create Decimal from f64"))
    }

    #[inline(always)]
    pub(crate) fn inner(self) -> Decimal {
        self.0
    }
}
