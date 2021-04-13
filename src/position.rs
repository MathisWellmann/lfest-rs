#[derive(Debug, Clone, Default)]
/// Describes the position information of the account
pub struct Position {
    /// The position size denoted in QUOTE currency
    size: f64,
    /// The value of the position, denoted in BASE currency
    value: f64,
    /// The entry price of the position
    entry_price: f64,
    /// The current position leverage
    leverage: f64,
    /// The currently unrealized profit and loss, denoted in BASE currency
    unrealized_pnl: f64,
}

impl Position {
    /// Create a new position with a given leverage
    pub fn new(leverage: f64) -> Self {
        Position {
            size: 0.0,
            value: 0.0,
            entry_price: 0.0,
            leverage,
            unrealized_pnl: 0.0,
        }
    }

    /// Create a new position with all fields custom.
    /// NOTE: only for advanced use cases
    pub fn new_all_fields(
        size: f64,
        value: f64,
        entry_price: f64,
        leverage: f64,
        unrealized_pnl: f64,
    ) -> Self {
        Position {
            size,
            value,
            entry_price,
            leverage,
            unrealized_pnl,
        }
    }

    /// Change the position size by a given delta, denoted in QUOTE currency at a given price
    pub(crate) fn change_size(&mut self, size_delta: f64, price: f64) {
        if self.size > 0.0 {
            if self.size + size_delta < 0.0 {
                // counts as new position as all old position size is sold
                self.entry_price = price;
            } else if (self.size + size_delta).abs() > self.size {
                self.entry_price = ((self.size.abs() * self.entry_price)
                    + (size_delta.abs() * price))
                    / (self.size.abs() + size_delta.abs());
            }
        } else if self.size < 0.0 {
            if self.size + size_delta > 0.0 {
                self.entry_price = price;
            } else if self.size + size_delta < self.size {
                self.entry_price = ((self.size.abs() * self.entry_price)
                    + (size_delta.abs() * price))
                    / (self.size.abs() + size_delta.abs());
            }
        } else {
            self.entry_price = price;
        }
        self.size += size_delta;

        self.update_state(price);
    }

    /// Update the state to reflect price changes
    pub(crate) fn update_state(&mut self, price: f64) {
        self.value = self.size.abs() / price;
        self.unrealized_pnl = if self.size != 0.0 {
            self.size * (1.0 / self.entry_price - 1.0 / price)
        } else {
            0.0
        };
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
    use crate::tests::round;

    #[test]
    fn position_change_size() {
        let mut pos = Position::new(1.0);

        pos.change_size(100.0, 100.0);
        assert_eq!(pos.size, 100.0);
        assert_eq!(pos.value, 1.0);
        assert_eq!(pos.entry_price, 100.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);

        pos.change_size(-50.0, 150.0);
        assert_eq!(pos.size, 50.0);
        assert_eq!(round(pos.value, 2), 0.33);
        assert_eq!(pos.entry_price, 100.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(round(pos.unrealized_pnl, 2), 0.17);

        pos.change_size(50.0, 150.0);
        assert_eq!(pos.size, 100.0);
        assert_eq!(round(pos.value, 2), 0.67);
        assert_eq!(pos.entry_price, 125.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(round(pos.unrealized_pnl, 2), 0.13);

        pos.change_size(-150.0, 150.0);
        assert_eq!(pos.size, -50.0);
        assert_eq!(round(pos.value, 2), 0.33);
        assert_eq!(pos.entry_price, 150.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);

        pos.change_size(50.0, 150.0);
        assert_eq!(pos.size, 0.0);
        assert_eq!(pos.value, 0.0);
        assert_eq!(pos.entry_price, 150.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);
    }

    #[test]
    fn position_update_state() {
        let mut pos = Position::new_all_fields(100.0, 1.0, 100.0, 1.0, 0.0);
        pos.update_state(110.0);

        assert_eq!(round(pos.value, 2), 0.91);
        assert_eq!(round(pos.unrealized_pnl, 2), 0.09);
    }
}
