use derive_more::Display;

use crate::types::errors::{Error, Result};

/// Allows the quick construction of `Leverage`
///
/// # Panics:
/// if a value < 1 is provided.
#[macro_export]
macro_rules! leverage {
    ( $a:literal ) => {{
        Leverage::new($a).expect("I have read the panic comment and know the leverage must be > 0.")
    }};
}

/// Leverage
#[derive(Default, Debug, Clone, Copy, PartialEq, Display, Eq)]
pub struct Leverage(u8);

impl Leverage {
    /// Create a new instance from a `Decimal` value
    #[inline]
    pub fn new(val: u8) -> Result<Self> {
        if val < 1 {
            Err(Error::InvalidLeverage)?
        }
        Ok(Self(val))
    }

    /// Get access to the inner `Decimal`
    #[inline(always)]
    pub fn inner(&self) -> u8 {
        self.0
    }
}
