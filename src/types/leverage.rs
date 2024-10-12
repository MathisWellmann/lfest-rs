use std::ops::Div;

use derive_more::Display;
use fpdec::{Dec, Decimal};

use super::{ConfigError, Mon};

/// Allows the quick construction of `Leverage`
///
/// # Panics:
/// if a value < 1 is provided.
#[macro_export]
macro_rules! leverage {
    ( $a:literal ) => {{
        $crate::prelude::Leverage::new($a)
            .expect("I have read the panic comment and know the leverage must be > 0.")
    }};
}

/// Leverage
#[derive(Default, Debug, Clone, Copy, PartialEq, Display, Eq)]
pub struct Leverage(u8);

impl Leverage {
    /// Create a new instance from a `Decimal` value
    pub fn new(val: u8) -> Result<Self, ConfigError> {
        if val < Dec!(1) {
            Err(ConfigError::InvalidLeverage)?
        }
        Ok(Self(val))
    }

    /// Compute the initial margin requirement from leverage.
    pub fn init_margin_req<T>(&self) -> T
    where
        T: Mon,
    {
        T::one() / T::from(self.0)
    }
}

impl Div<Leverage> for Decimal {
    type Output = Decimal;

    fn div(self, rhs: Leverage) -> Self::Output {
        self / Decimal::from(rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_leverage() {
        assert_eq!(std::mem::size_of::<Leverage>(), 1);
    }
}
