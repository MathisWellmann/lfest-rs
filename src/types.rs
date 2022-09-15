use std::fmt::Formatter;

use crate::{Error, Result};

/// Decribes the possible updates to the market state
#[derive(Debug, Clone, PartialEq)]
pub enum MarketUpdate {
    /// An update to the best bid and ask has occured
    Bba {
        /// The new best bid
        bid: f64,
        /// The new best ask
        ask: f64,
    },
    /// A new candle has been created
    Candle {
        /// The best bid at the time of candle creation
        bid: f64,
        /// The best ask at the time of candle creation
        ask: f64,
        /// The low price of the candle
        low: f64,
        /// The high price of the candle
        high: f64,
    },
}

/// Creates the MarketUpdate::Bba variant
#[macro_export]
macro_rules! bba {
    ( $b:expr, $a:expr ) => {{
        MarketUpdate::Bba {
            bid: $b,
            ask: $a,
        }
    }};
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
/// Side of the order
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

impl Side {
    #[inline(always)]
    /// Return the integer representation of this enum
    pub fn as_integer(&self) -> u64 {
        match self {
            Side::Buy => 0,
            Side::Sell => 1,
        }
    }

    #[inline(always)]
    /// Parse the Side from an integer value
    pub fn from_integer(val: u64) -> Result<Self> {
        match val {
            0 => Ok(Side::Buy),
            1 => Ok(Side::Sell),
            _ => Err(Error::ParseError),
        }
    }

    /// Returns the inverted side
    pub fn inverted(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}

impl OrderType {
    /// Return the integer representation of this enum
    #[deprecated]
    #[inline(always)]
    pub fn as_integer(&self) -> u64 {
        match self {
            OrderType::Market => 0,
            OrderType::Limit => 1,
        }
    }

    /// Parse the OrderType from integer value
    #[inline(always)]
    #[deprecated]
    pub fn from_integer(val: u64) -> Result<Self> {
        match val {
            0 => Ok(Self::Market),
            1 => Ok(Self::Limit),
            _ => Err(Error::ParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bba_macro() {
        let m = bba!(100.0, 100.1);

        assert_eq!(
            m,
            MarketUpdate::Bba {
                bid: 100.0,
                ask: 100.1
            }
        );
    }
}
