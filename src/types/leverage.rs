use const_decimal::Decimal;
use derive_more::Display;
use num_traits::One;

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
        if val < 1 {
            Err(ConfigError::InvalidLeverage)?
        }
        Ok(Self(val))
    }

    /// Compute the initial margin requirement from leverage.
    pub fn init_margin_req<I, const D: u8>(&self) -> Decimal<I, D>
    where
        I: Mon<D>,
    {
        Decimal::one() / Decimal::try_from_scaled(I::from(self.0).unwrap(), 1).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;

    use super::*;

    #[test]
    fn size_of_leverage() {
        assert_eq!(std::mem::size_of::<Leverage>(), 1);
    }

    #[test]
    fn leverage_init_margin_req() {
        assert_eq!(Leverage(1).init_margin_req::<i32, 2>(), Decimal::one());
        assert_eq!(
            Leverage(2).init_margin_req::<i32, 2>(),
            Decimal::one() / Decimal::TWO
        );
    }
}
