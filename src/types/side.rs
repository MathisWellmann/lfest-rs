use std::fmt::Formatter;

use fpdec::{Dec, Decimal};

use super::Currency;

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
    pub fn from_taker_quantity<Q: Currency>(qty: Q) -> Self {
        assert_ne!(*qty.as_ref(), Dec!(0), "A trade quantity cannot be zero");

        if *qty.as_ref() < Dec!(0) {
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
    use super::*;
    use crate::quote;

    #[test]
    fn side_from_taker_quantity() {
        assert_eq!(Side::from_taker_quantity(quote!(1)), Side::Buy);
        assert_eq!(Side::from_taker_quantity(quote!(-1)), Side::Sell);
    }

    #[test]
    #[should_panic]
    fn side_from_taker_quantity_panic() {
        Side::from_taker_quantity(quote!(0));
    }
}
