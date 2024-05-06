use derive_more::Display;
use fpdec::Decimal;

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
    pub fn new(val: Decimal) -> Self {
        Self(val)
    }

    /// Get access to the inner `Decimal`
    #[inline(always)]
    pub fn inner(self) -> Decimal {
        self.0
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
