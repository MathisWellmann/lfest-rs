use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Describes the margin information of the account
pub struct Margin {
    /// The wallet balance of account
    wallet_balance: f64,
    /// The position margin of account
    position_margin: f64,
    /// The order margin of account
    order_margin: f64,
    /// The available balance of account
    available_balance: f64,
}

impl Margin {
    /// Create a new margin account with an initial balance
    pub fn new_init(init_balance: f64) -> Self {
        debug_assert!(init_balance > 0.0);
        Margin {
            wallet_balance: init_balance,
            position_margin: 0.0,
            order_margin: 0.0,
            available_balance: init_balance,
        }
    }

    /// Create a new Margin with all fields custom
    pub fn new(
        wallet_balance: f64,
        position_margin: f64,
        order_margin: f64,
        available_balance: f64,
    ) -> Self {
        debug_assert!(wallet_balance.is_finite());
        debug_assert!(position_margin.is_finite());
        debug_assert!(order_margin.is_finite());
        debug_assert!(available_balance.is_finite());
        Margin {
            wallet_balance,
            position_margin,
            order_margin,
            available_balance,
        }
    }

    /// Set a new order margin
    pub(crate) fn set_order_margin(&mut self, order_margin: f64) {
        debug_assert!(order_margin >= 0.0);

        self.order_margin = order_margin;
        self.available_balance = self.wallet_balance - self.position_margin - self.order_margin;

        debug!(
            "self.available_balance: {}, self.wallet_balance: {}, self.position_margin: {}, self.order_margin: {}",
            self.available_balance,
            self.wallet_balance,
            self.position_margin,
            self.order_margin
        );
        debug_assert!(self.available_balance >= 0.0);
    }

    /// Set the position margin by a given delta and adjust available balance accordingly
    pub(crate) fn set_position_margin(&mut self, val: f64) {
        trace!("set_position_margin({})", val);

        debug_assert!(val >= 0.0);

        self.position_margin = val;
        self.available_balance = self.wallet_balance - self.order_margin - self.position_margin;

        debug_assert!(self.position_margin >= 0.0);
        debug_assert!(self.position_margin <= self.wallet_balance);
        debug_assert!(self.available_balance >= 0.0);
        debug_assert!(self.available_balance <= self.wallet_balance);
    }

    /// Change the balance by a given delta e.g. from realizing profit or loss
    /// Return true if successful
    pub(crate) fn change_balance(&mut self, delta: f64) {
        self.wallet_balance += delta;
        self.available_balance += delta;

        // debug_assert!(self.wallet_balance >= 0.0);
        // debug_assert!(self.available_balance >= 0.0);
    }

    /// Return the wallet balance of account
    pub fn wallet_balance(&self) -> f64 {
        self.wallet_balance
    }

    /// Return the position margin of account
    pub fn position_margin(&self) -> f64 {
        self.position_margin
    }

    /// Return the used order margin of account
    pub fn order_margin(&self) -> f64 {
        self.order_margin
    }

    /// Return the available balance of account
    pub fn available_balance(&self) -> f64 {
        self.available_balance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margin_set_order_margin() {
        let mut margin = Margin::new_init(1.0);
        margin.set_order_margin(1.0);
        assert_eq!(margin.wallet_balance, 1.0);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 1.0);
        assert_eq!(margin.available_balance, 0.0);
    }

    #[test]
    fn margin_change_balance() {
        let mut margin = Margin::new_init(1.0);

        margin.change_balance(0.05);
        assert_eq!(margin.wallet_balance, 1.05);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(margin.available_balance, 1.05);
    }
}
