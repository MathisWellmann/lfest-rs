use derive_more::Display;
use fpdec::Decimal;

/// Allows the quick construction of `Fee`
#[macro_export]
macro_rules! fee {
    ( $a:literal) => {{
        Fee::new(fpdec::Dec!($a))
    }};
}

/// Fee as a fraction
#[derive(Default, Debug, Clone, Copy, PartialEq, Display, Serialize, Deserialize)]
pub struct Fee(Decimal);

impl Fee {
    /// Create a new instance from a `Decimal` value
    #[inline(always)]
    pub fn new(val: Decimal) -> Self {
        Self(val)
    }

    #[inline(always)]
    pub(crate) fn inner(self) -> Decimal {
        self.0
    }
}
