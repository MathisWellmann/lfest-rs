use derive_more::Display;

use super::{BasisPointFrac, ConfigError};

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
    pub fn init_margin_req(&self) -> BasisPointFrac {
        // Decimal::one() / Decimal::try_from_scaled(self.0 as i32, 1).unwrap()
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_leverage() {
        assert_eq!(std::mem::size_of::<Leverage>(), 1);
    }

    #[test]
    fn leverage_init_margin_req() {
        assert_eq!(Leverage(1).init_margin_req(), BasisPointFrac::one());
        assert_eq!(
            Leverage(2).init_margin_req(),
            BasisPointFrac::one() / BasisPointFrac::TWO
        );
    }
}
