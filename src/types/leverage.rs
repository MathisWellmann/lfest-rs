use std::ops::Div;

use derive_more::Display;
use fpdec::{Dec, Decimal};

use super::{BaseCurrency, ConfigError, Currency, QuoteCurrency};

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
}

impl Div<Leverage> for Decimal {
    type Output = Decimal;

    fn div(self, rhs: Leverage) -> Self::Output {
        self / Decimal::from(rhs.0)
    }
}

impl Div<Leverage> for BaseCurrency {
    type Output = Self;

    fn div(self, rhs: Leverage) -> Self::Output {
        BaseCurrency::new(self.as_ref() / Decimal::from(rhs.0))
    }
}

impl Div<Leverage> for QuoteCurrency {
    type Output = Self;

    fn div(self, rhs: Leverage) -> Self::Output {
        QuoteCurrency::new(self.as_ref() / Decimal::from(rhs.0))
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
