use serde::{Deserialize, Serialize};

use crate::FuturesTypes;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
/// Describes the position information of the account
pub struct Position {
    /// The position size
    /// denoted in BASE currency if using linear futures,
    /// denoted in QUOTE currency if using inverse futures
    size: f64,
    /// The entry price of the position
    entry_price: f64,
    /// The current position leverage
    leverage: f64,
    /// The currently unrealized profit and loss
    unrealized_pnl: f64,
}

impl Position {
    /// Create a new, initially neutral, position with a given leverage
    /// # Panics
    /// In debug mode, if leverage is smaller than 1.0
    #[must_use]
    #[inline]
    pub fn new_init(leverage: f64) -> Self {
        debug_assert!(leverage >= 1.0);
        Position {
            size: 0.0,
            entry_price: 0.0,
            leverage,
            unrealized_pnl: 0.0,
        }
    }

    /// Create a new position with all fields custom.
    /// NOTE: not usually called, but for advanced use cases
    /// # Panics
    /// In debug mode, if inputs don't make sense
    #[must_use]
    #[inline]
    pub fn new(size: f64, entry_price: f64, leverage: f64, unrealized_pnl: f64) -> Self {
        debug_assert!(size.is_finite());
        debug_assert!(entry_price.is_finite());
        debug_assert!(leverage.is_finite());
        debug_assert!(unrealized_pnl.is_finite());

        debug_assert!(leverage >= 1.0);
        debug_assert!(entry_price >= 0.0);

        Position {
            size,
            entry_price,
            leverage,
            unrealized_pnl,
        }
    }

    /// Return the position size
    /// denoted in BASE currency if using linear futures,
    /// denoted in QUOTE currency if using inverse futures
    #[inline(always)]
    pub fn size(&self) -> f64 {
        self.size
    }

    /// Return the entry price of the position
    #[inline(always)]
    pub fn entry_price(&self) -> f64 {
        self.entry_price
    }

    /// Return the positions leverage
    #[inline(always)]
    pub fn leverage(&self) -> f64 {
        self.leverage
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    #[inline(always)]
    pub fn unrealized_pnl(&self) -> f64 {
        self.unrealized_pnl
    }

    /// Change the position size by a given delta at a certain price
    pub(crate) fn change_size(&mut self, size_delta: f64, price: f64, futures_type: FuturesTypes) {
        debug!("change_size({}, {}, {})", size_delta, price, futures_type);

        debug_assert!(price > 0.0);

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

        self.update_state(price, futures_type);
    }

    /// Update the state to reflect price changes
    #[inline]
    pub(crate) fn update_state(&mut self, price: f64, futures_type: FuturesTypes) {
        self.unrealized_pnl = if self.size != 0.0 {
            futures_type.pnl(self.entry_price, price, self.size)
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::round;

    #[test]
    fn position_change_size() {
        let mut pos = Position::new_init(1.0);
        let futures_type = FuturesTypes::Inverse;

        pos.change_size(100.0, 100.0, futures_type);
        assert_eq!(pos.size, 100.0);
        assert_eq!(pos.entry_price, 100.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);

        pos.change_size(-50.0, 150.0, futures_type);
        assert_eq!(pos.size, 50.0);
        assert_eq!(pos.entry_price, 100.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(round(pos.unrealized_pnl, 2), 0.17);

        pos.change_size(50.0, 150.0, futures_type);
        assert_eq!(pos.size, 100.0);
        assert_eq!(pos.entry_price, 125.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(round(pos.unrealized_pnl, 2), 0.13);

        pos.change_size(-150.0, 150.0, futures_type);
        assert_eq!(pos.size, -50.0);
        assert_eq!(pos.entry_price, 150.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);

        pos.change_size(50.0, 150.0, futures_type);
        assert_eq!(pos.size, 0.0);
        assert_eq!(pos.entry_price, 150.0);
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, 0.0);
    }

    #[test]
    fn position_update_state_inverse_futures() {
        let mut pos = Position::new(100.0, 100.0, 1.0, 0.0);
        pos.update_state(110.0, FuturesTypes::Inverse);

        assert_eq!(round(pos.unrealized_pnl, 2), 0.09);
    }

    #[test]
    fn position_update_state_linear_futures() {
        let mut pos = Position::new(1.0, 100.0, 1.0, 0.0);
        pos.update_state(110.0, FuturesTypes::Linear);

        assert_eq!(round(pos.unrealized_pnl, 2), 10.0);
    }
}
