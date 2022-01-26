use crate::errors::{Error, Result};
use std::fmt::Formatter;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Enumeration of different futures types
pub enum FuturesTypes {
    /// Linear futures with a linear payout
    /// profit and loss calculation: position_size * (exit_price - entry_price)
    Linear,

    /// Inverse futures allow the user to hold the collateral in BASE currency and speculating on price moves denoted in QUOTE currency
    /// Example would be Bitmex XBTUSD inverse perpetual futures.
    /// profit and loss calculation: position_size * (1.0 / entry_price - 1.0 / exit_price)
    Inverse,
}

impl std::fmt::Display for FuturesTypes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for FuturesTypes {
    fn default() -> Self {
        Self::Linear
    }
}

impl FuturesTypes {
    /// return the profit and loss for a given entry and exit price with a given contract_qty
    /// Note that negative contract_qty will give the pnl for a short position
    #[inline]
    pub fn pnl(&self, entry_price: f64, exit_price: f64, contract_qty: f64) -> f64 {
        match self {
            Self::Linear => {
                // contract_qty is denoted in BASE currency
                contract_qty * (exit_price - entry_price)
                // resulting pnl denoted in QUOTE currency
            }
            Self::Inverse => {
                // contract_qty is denoted in QUOTE currency
                contract_qty * (1.0 / entry_price - 1.0 / exit_price)
                // resulting pnl denoted in BASE currency
            }
        }
    }

    /// Parse the FuturesType from a string
    #[inline]
    pub fn from_str(s: &str) -> Result<Self> {
        if s.to_uppercase() == "LINEAR" {
            Ok(Self::Linear)
        } else if s.to_uppercase() == "INVERSE" {
            Ok(Self::Inverse)
        } else {
            Err(Error::ParseError)
        }
    }

    /// String representation of the FuturesType
    #[inline(always)]
    pub fn to_str(&self) -> &'static str {
        match self {
            FuturesTypes::Linear => "LINEAR",
            FuturesTypes::Inverse => "INVERSE",
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::round;

    #[test]
    fn futures_type_pnl_linear() {
        let ft = FuturesTypes::Linear;

        assert_eq!(ft.pnl(100.0, 110.0, 10.0), 100.0);
        assert_eq!(ft.pnl(100.0, 110.0, -10.0), -100.0);
        assert_eq!(ft.pnl(100.0, 90.0, 10.0), -100.0);
        assert_eq!(ft.pnl(100.0, 90.0, -10.0), 100.0);
    }

    #[test]
    fn futures_type_pnl_inverse() {
        let ft = FuturesTypes::Inverse;

        assert_eq!(round(ft.pnl(100.0, 110.0, 1000.0), 3), 0.909);
        assert_eq!(round(ft.pnl(100.0, 110.0, -1000.0), 3), -0.909);
        assert_eq!(round(ft.pnl(100.0, 90.0, 1000.0), 3), -1.111);
        assert_eq!(round(ft.pnl(100.0, 90.0, -1000.0), 3), 1.111);
    }
}
