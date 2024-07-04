use derive_more::Display;
use fpdec::{Dec, Decimal};

use super::ConfigError;

/// Allows the quick construction of `Leverage`
///
/// # Panics:
/// if a value < 1 is provided.
#[macro_export]
macro_rules! leverage {
    ( $a:literal ) => {{
        use $crate::prelude::fpdec::Decimal;
        $crate::prelude::Leverage::new($crate::prelude::fpdec::Dec!($a))
            .expect("I have read the panic comment and know the leverage must be > 0.")
    }};
}

/// Leverage
#[derive(Default, Debug, Clone, Copy, PartialEq, Display, Eq)]
pub struct Leverage(Decimal);

impl Leverage {
    /// Create a new instance from a `Decimal` value
    pub fn new(val: Decimal) -> Result<Self, ConfigError> {
        if val < Dec!(1) {
            Err(ConfigError::InvalidLeverage)?
        }
        Ok(Self(val))
    }
}

impl AsRef<Decimal> for Leverage {
    fn as_ref(&self) -> &Decimal {
        &self.0
    }
}
