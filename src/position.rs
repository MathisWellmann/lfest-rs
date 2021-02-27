#[derive(Debug, Clone, Default)]
/// Describes the position information of the account
pub struct Position {
    /// The position size denoted in QUOTE currency
    pub(crate) size: f64,
    /// The value of the position, denoted in BASE currency
    pub(crate) value: f64,
    /// The entry price of the position
    pub(crate) entry_price: f64,
    /// The liquidation price of the position
    pub(crate) liq_price: f64,
    /// The margin used for this position
    pub(crate) margin: f64,
    /// The current position leverage
    pub(crate) leverage: f64,
    /// The currently unrealized profit and loss, denoted in BASE currency
    pub(crate) unrealized_pnl: f64,
}

impl Position {
    /// Create a new position with a given leverage
    pub fn new(leverage: f64) -> Self {
        Position {
            size: 0.0,
            value: 0.0,
            entry_price: 0.0,
            liq_price: 0.0,
            margin: 0.0,
            leverage,
            unrealized_pnl: 0.0
        }
    }

    /// Change the position size by a given delta, denoted in QUOTE currency at a given price
    pub fn change_size(&mut self, size_delta: f64, price: f64) {
        // TODO:

        let margin: f64 = size_delta / price / self.leverage;
        self.margin += margin;
        self.size += size_delta;

        self.value = self.size.abs() / self.entry_price;
        self.update_upnl(price);
    }

    /// Update the unrealized profit and loss calculation
    pub fn update_upnl(&mut self, price: f64) {
        self.unrealized_pnl = self.size * (1.0 / self.entry_price - 1.0 / price);
    }

    /// Return the position size denoted in QUOTE currency
    pub fn size(&self) -> f64 {
        self.size
    }

    /// Return the position value denoted in BASE currency
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Return the entry price of the position
    pub fn entry_price(&self) -> f64 {
        self.entry_price
    }

    /// Return the liquidation price of the position
    pub fn liq_price(&self) -> f64 {
        self.liq_price
    }

    /// Return the currently used margin for this position, denoted in BASE currency
    pub fn margin(&self) -> f64 {
        self.margin
    }

    /// Return the positions leverage
    pub fn leverage(&self) -> f64 {
        self.leverage
    }

    /// Return the positions unrealized profit and loss, denoted in BASE currency
    pub fn unrealized_pnl(&self) -> f64 {
        self.unrealized_pnl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn position_change_size() {
        let mut pos = Position::new(1.0);

        pos.change_size(100.0, 100.0);
        assert_eq!(pos.size, 100.0);
        assert_eq!(pos.value, 1.0);
        assert_eq!(pos.entry_price, 100.0);
        assert_eq!(pos.liq_price, 0.0);
        assert_eq!(pos.margin, 1.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);
    }

    #[test]
    fn position_update_profit_and_loss() {
        let mut pos = Position::new(1.0);
        // TODO:
    }
}