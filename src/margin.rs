use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Describes the margin information of the account
pub struct Margin {
    /// The wallet balance of account
    /// denoted in QUOTE currency when using linear futures
    /// denoted in BASE currency when using inverse futures
    wallet_balance: f64,
    /// The position margin of account, same denotation as wallet_balance
    position_margin: f64,
    /// The order margin of account, same denotation as wallet_balance
    order_margin: f64,
    /// The available balance of account, same denotation as wallet_balance
    available_balance: f64,
}

impl Margin {
    /// Create a new margin account with an initial balance
    /// # Panics
    //  In debug mode, if the input values don't make sense
    #[must_use]
    #[inline]
    pub fn new_init(init_balance: f64) -> Self {
        debug_assert!(init_balance > 0.0);
        debug_assert!(init_balance.is_finite());

        Margin {
            wallet_balance: init_balance,
            position_margin: 0.0,
            order_margin: 0.0,
            available_balance: init_balance,
        }
    }

    /// Create a new Margin with all fields custom
    /// # Panics
    /// In debug mode, if the input values don't make sense
    #[must_use]
    #[inline]
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

        debug_assert!(wallet_balance > 0.0);
        debug_assert!(position_margin >= 0.0);
        debug_assert!(order_margin >= 0.0);
        debug_assert!(available_balance >= 0.0);

        debug_assert!(position_margin <= wallet_balance);
        debug_assert!(order_margin <= wallet_balance);
        debug_assert!(available_balance <= wallet_balance);

        Margin {
            wallet_balance,
            position_margin,
            order_margin,
            available_balance,
        }
    }

    /// Return the wallet balance of account
    /// denoted in QUOTE currency when using linear futures
    /// denoted in BASE currency when using inverse futures
    #[inline(always)]
    pub fn wallet_balance(&self) -> f64 {
        self.wallet_balance
    }

    /// Return the position margin of account, same denotation as wallet_balance
    #[inline(always)]
    pub fn position_margin(&self) -> f64 {
        self.position_margin
    }

    /// Return the used order margin of account, same denotation as wallet_balance
    #[inline(always)]
    pub fn order_margin(&self) -> f64 {
        self.order_margin
    }

    /// Return the available balance of account, same denotation as wallet_balance
    #[inline(always)]
    pub fn available_balance(&self) -> f64 {
        self.available_balance
    }

    /// Set a new order margin
    #[inline]
    pub(crate) fn set_order_margin(&mut self, om: f64) {
        debug_assert!(om >= 0.0);
        debug_assert!(om.is_finite());

        debug!("set_order_margin: om: {}, self: {:?}", om, self);

        self.order_margin = om;
        self.available_balance = self.wallet_balance - self.position_margin - self.order_margin;

        debug_assert!(self.available_balance >= 0.0);
    }

    /// Set the position margin by a given delta and adjust available balance accordingly
    #[inline]
    pub(crate) fn set_position_margin(&mut self, val: f64) {
        debug!("set_position_margin({}), self: {:?}", val, self);

        debug_assert!(val.is_finite());
        debug_assert!(val >= 0.0);

        self.position_margin = val;
        self.available_balance = self.wallet_balance - self.order_margin - self.position_margin;

        debug_assert!(self.position_margin >= 0.0);
        debug_assert!(self.position_margin <= self.wallet_balance);
        debug_assert!(self.available_balance >= 0.0);
        debug_assert!(self.available_balance <= self.wallet_balance);
    }

    /// Change the balance by a given delta e.g. from realizing profit or loss
    #[inline]
    pub(crate) fn change_balance(&mut self, delta: f64) {
        debug_assert!(delta.is_finite());

        debug!("change_balance: delta: {}, self: {:?}", delta, self);

        self.wallet_balance += delta;
        self.available_balance += delta;

        debug_assert!(self.wallet_balance >= 0.0);
        debug_assert!(self.available_balance >= 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::round;

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
    #[should_panic]
    fn margin_set_order_margin_panic_0() {
        let mut margin = Margin::new_init(1.0);
        margin.set_order_margin(1.01);
    }

    #[test]
    #[should_panic]
    fn margin_set_order_margin_panic_1() {
        let mut margin = Margin::new_init(1.0);
        margin.set_order_margin(-0.1);
    }

    #[test]
    fn margin_set_position_margin() {
        let mut margin = Margin::new_init(1.0);
    }

    #[test]
    fn margin_change_balance() {
        let mut margin = Margin::new_init(1.0);

        margin.change_balance(0.05);
        assert_eq!(margin.wallet_balance, 1.05);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(margin.available_balance, 1.05);

        margin.change_balance(-0.1);
        assert_eq!(round(margin.wallet_balance, 2), 0.95);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(round(margin.available_balance, 2), 0.95);
    }

    #[test]
    #[should_panic]
    fn margin_change_balance_panic_0() {
        let mut margin = Margin::new_init(1.0);

        margin.change_balance(-1.01);
    }
}
