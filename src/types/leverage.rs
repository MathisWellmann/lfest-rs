use derive_more::Display;

/// Allows the quick construction of `Leverage`
#[macro_export]
macro_rules! leverage {
    ( $a:literal ) => {{
        Leverage::new($a)
    }};
}

/// Leverage
#[derive(Default, Debug, Clone, Copy, PartialEq, Display, Eq)]
pub struct Leverage(u8);

impl Leverage {
    /// Create a new instance from a `Decimal` value
    #[inline(always)]
    pub fn new(val: u8) -> Self {
        debug_assert!(val > 0);
        Self(val)
    }

    /// Get access to the inner `Decimal`
    #[inline(always)]
    pub fn inner(&self) -> u8 {
        self.0
    }
}
