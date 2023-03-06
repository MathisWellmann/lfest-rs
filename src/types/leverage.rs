use derive_more::Display;
use fpdec::Decimal;

/// Allows the quick construction of `Leverage`
#[macro_export]
macro_rules! leverage {
    ( $a:literal ) => {{
        Leverage::new(fpdec::Dec!($a))
    }};
}

/// Leverage
/// TODO: Change this to u8 type, as no fractional leverage should be possible
#[derive(Default, Debug, Clone, Copy, PartialEq, Display, Serialize, Deserialize)]
pub struct Leverage(Decimal);

impl Leverage {
    /// Create a new instance from a `Decimal` value
    #[inline(always)]
    pub fn new(val: Decimal) -> Self {
        Self(val)
    }

    /// Get access to the inner `Decimal`
    #[inline(always)]
    pub fn inner(&self) -> Decimal {
        self.0
    }
}
