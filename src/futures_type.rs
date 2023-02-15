use std::{fmt::Formatter, str::FromStr};

use crate::{
    errors::{Error, Result},
    Currency, QuoteCurrency,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Enumeration of different futures types
pub enum FuturesTypes {
    /// Linear futures with a linear payout
    /// profit and loss calculation: position_size * (exit_price - entry_price)
    Linear,

    /// Inverse futures allow the user to hold the collateral in BASE currency
    /// and speculating on price moves denoted in QUOTE currency
    /// Example would be Bitmex XBTUSD inverse perpetual futures.
    /// profit and loss calculation: position_size * (1.0 / entry_price - 1.0 /
    /// exit_price)
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
    /// Return the profit and loss for a given entry and exit price with a given
    /// `contract_qty`.
    /// Note that negative `contract_qty` will give the pnl for a
    /// short position.
    #[inline]
    pub fn pnl<S>(
        &self,
        entry_price: QuoteCurrency,
        exit_price: QuoteCurrency,
        contract_qty: S,
    ) -> S::PairedCurrency
    where
        S: Currency,
    {
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

    /// String representation of the FuturesType
    #[inline(always)]
    pub fn to_str(&self) -> &'static str {
        match self {
            FuturesTypes::Linear => "LINEAR",
            FuturesTypes::Inverse => "INVERSE",
        }
    }
}

impl FromStr for FuturesTypes {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.to_uppercase() == "LINEAR" {
            Ok(Self::Linear)
        } else if s.to_uppercase() == "INVERSE" {
            Ok(Self::Inverse)
        } else {
            Err(Error::ParseError)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{base, quote, BaseCurrency};

    #[test]
    fn futures_type_pnl_linear() {
        let ft = FuturesTypes::Linear;

        assert_eq!(ft.pnl(quote!(100.0), quote!(110.0), base!(10.0)), quote!(100.0));
        assert_eq!(ft.pnl(quote!(100.0), quote!(110.0), base!(-10.0)), quote!(-100.0));
        assert_eq!(ft.pnl(quote!(100.0), quote!(90.0), base!(10.0)), quote!(-100.0));
        assert_eq!(ft.pnl(quote!(100.0), quote!(90.0), base!(-10.0)), quote!(100.0));
    }

    #[test]
    fn futures_type_pnl_inverse() {
        let ft = FuturesTypes::Inverse;

        // TODO: remove rounding and use better float type
        assert_eq!(
            ft.pnl(quote!(100.0), quote!(110.0), quote!(1000.0)).into_rounded(3),
            base!(0.909)
        );
        assert_eq!(
            ft.pnl(quote!(100.0), quote!(110.0), quote!(-1000.0)).into_rounded(3),
            base!(-0.909)
        );
        assert_eq!(
            ft.pnl(quote!(100.0), quote!(90.0), quote!(1000.0)).into_rounded(3),
            base!(-1.111)
        );
        assert_eq!(
            ft.pnl(quote!(100.0), quote!(90.0), quote!(-1000.0)).into_rounded(3),
            base!(1.111)
        );
    }
}
