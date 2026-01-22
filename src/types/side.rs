use std::fmt::Formatter;

use Side::*;
use serde::{
    Deserialize,
    Serialize,
};

/// Side of the order
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Side {
    /// Buy side
    Buy = 0,
    /// Sell side
    Sell = 1,
}

impl Side {
    /// Returns the inverted side
    #[inline(always)]
    pub fn inverted(&self) -> Self {
        match self {
            Buy => Sell,
            Sell => Buy,
        }
    }

    /// Parse the side of a taker trade from the trade quantity.
    pub fn from_taker_quantity<BaseOrQuote>(qty: BaseOrQuote) -> Self
    where
        BaseOrQuote: num_traits::Signed,
    {
        assert!(!qty.is_zero(), "A trade quantity cannot be zero");

        if qty.is_negative() { Sell } else { Buy }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use num_traits::Zero;

    use super::*;
    use crate::prelude::QuoteCurrency;

    #[test]
    fn side_from_taker_quantity() {
        assert_eq!(
            Side::from_taker_quantity(QuoteCurrency::<i32, 4>::new(1, 0)),
            Buy
        );
        assert_eq!(
            Side::from_taker_quantity(QuoteCurrency::<i32, 4>::new(-1, 0)),
            Sell
        );
    }

    #[test]
    #[should_panic]
    fn side_from_taker_quantity_panic() {
        Side::from_taker_quantity(QuoteCurrency::<i64, 4>::zero());
    }

    #[test]
    fn side_display() {
        assert_eq!(&Buy.to_string(), "Buy");
        assert_eq!(&Sell.to_string(), "Sell");
    }

    #[test]
    fn size_of_side() {
        assert_eq!(size_of::<Side>(), 1);
    }
}
