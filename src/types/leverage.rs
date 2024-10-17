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
pub struct Leverage<I, const D: u8>(Decimal<I, D>);

impl<I, const D: u8> Leverage<I, D>
where
    I: Mon<D>,
{
    /// Create a new instance from a `Decimal` value
    pub fn new(val: u8) -> Result<Self, ConfigError> {
        if val < 1 {
            Err(ConfigError::InvalidLeverage)?
        }
        Ok(Self(
            Decimal::try_from_scaled(I::from(val).expect("u8 leverage can convert to I"), 0)
                .expect("Can create `Decimal` for `Leverage`"),
        ))
    }

    /// Compute the initial margin requirement from leverage.
    #[inline]
    pub fn init_margin_req(&self) -> Decimal<I, D> {
        Decimal::one() / self.0
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;

    use super::*;

    #[test]
    fn size_of_leverage() {
        assert_eq!(std::mem::size_of::<Leverage<i32, 0>>(), 4);
        assert_eq!(std::mem::size_of::<Leverage<i64, 0>>(), 8);
    }

    #[test]
    fn leverage() {
        for i in 1..100 {
            let _ = Leverage::<i32, 0>::new(i).unwrap();
            let _ = Leverage::<i64, 0>::new(i).unwrap();
        }
    }

    #[test]
    fn leverage_init_margin_req() {
        assert_eq!(
            Leverage::<i32, 0>::new(1).unwrap().init_margin_req(),
            Decimal::one()
        );
        assert_eq!(
            Leverage::<i32, 0>::new(2).unwrap().init_margin_req(),
            Decimal::one() / Decimal::TWO
        );
    }
}
