use std::fmt::Formatter;

/// Side of the order
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

impl Side {
    /// Returns the inverted side
    pub fn inverted(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }

    /// Parse the side of a taker trade from the trade quantity.
    pub fn from_taker_quantity<BaseOrQuote>(qty: BaseOrQuote) -> Self
    where
        BaseOrQuote: num_traits::Signed,
    {
        assert!(!qty.is_zero(), "A trade quantity cannot be zero");

        if qty.is_negative() {
            Side::Sell
        } else {
            Side::Buy
        }
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
            Side::from_taker_quantity(QuoteCurrency::<i32, 4, 2>::new(1, 0)),
            Side::Buy
        );
        assert_eq!(
            Side::from_taker_quantity(QuoteCurrency::<i32, 4, 2>::new(-1, 0)),
            Side::Sell
        );
    }

    #[test]
    #[should_panic]
    fn side_from_taker_quantity_panic() {
        Side::from_taker_quantity(QuoteCurrency::<i64, 4, 2>::zero());
    }
}
