#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Enumeration of different futures types
pub enum FuturesType {
    /// Linear futures with a linear payout
    /// profit and loss calculation: position_size * (exit_price - entry_price)
    Linear,

    /// Inverse futures allow the user to hold the collateral in BASE currency and speculating on price moves denoted in QUOTE currency
    /// Example would be Bitmex XBTUSD inverse perpetual futures.
    /// profit and loss calculation: position_size * (1.0 / entry_price - 1.0 / exit_price)
    Inverse,
}

impl Default for FuturesType {
    fn default() -> Self {
        Self::Linear
    }
}

impl FuturesType {
    /// return the profit and loss for a given entry and exit price with a given contract_qty
    /// Note that negative contract_qty will give the pnl for a short position
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
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::round;

    #[test]
    fn futures_type_pnl() {
        let ft = FuturesType::Linear;
        let entry_price: f64 = 100.0;
        let exit_price: f64 = 110.0;

        assert_eq!(ft.pnl(entry_price, exit_price, 10.0), 100.0);
        assert_eq!(ft.pnl(entry_price, exit_price, -10.0), -100.0);

        let ft = FuturesType::Inverse;
        let entry_price: f64 = 100.0;
        let exit_price: f64 = 110.0;

        assert_eq!(round(ft.pnl(entry_price, exit_price, 10.0), 5), 0.00909);
        assert_eq!(round(ft.pnl(entry_price, exit_price, -10.0), 5), -0.00909);
    }
}
