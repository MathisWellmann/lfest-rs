use serde::{Deserialize, Serialize};

use crate::{
    errors::{Error, Result},
    types::Currency,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Describes the margin information of the account
pub struct Margin<M> {
    /// The wallet balance of account
    /// denoted in QUOTE currency when using linear futures
    /// denoted in BASE currency when using inverse futures
    wallet_balance: M,
    /// The position margin of account, same denotation as wallet_balance
    position_margin: M,
    /// The order margin of account, same denotation as wallet_balance
    order_margin: M,
    /// The available balance of account, same denotation as wallet_balance
    available_balance: M,
}

impl<M> Margin<M>
where M: Currency
{
    /// Create a new margin account with an initial balance
    /// # Panics
    //  In debug mode, if the input values don't make sense
    #[must_use]
    #[inline]
    pub fn new_init(init_balance: M) -> Self {
        Margin {
            wallet_balance: init_balance,
            position_margin: M::new_zero(),
            order_margin: M::new_zero(),
            available_balance: init_balance,
        }
    }

    /// Create a new Margin with all fields custom.
    ///
    /// # Panics:
    /// In debug mode, if the input values don't make sense
    #[inline]
    pub fn new(
        wallet_balance: M,
        position_margin: M,
        order_margin: M,
        available_balance: M,
    ) -> Result<Self> {
        if position_margin > wallet_balance {
            return Err(Error::InvalidPositionMargin);
        }
        if order_margin > wallet_balance {
            return Err(Error::InvalidOrderMargin);
        }
        if available_balance > wallet_balance {
            return Err(Error::InvalidAvailableBalance);
        }

        Ok(Margin {
            wallet_balance,
            position_margin,
            order_margin,
            available_balance,
        })
    }

    /// Return the wallet balance of account
    #[inline(always)]
    pub fn wallet_balance(&self) -> M {
        self.wallet_balance
    }

    /// Return the position margin of account, same denotation as wallet_balance
    #[inline(always)]
    pub fn position_margin(&self) -> M {
        self.position_margin
    }

    /// Return the used order margin of account, same denotation as
    /// wallet_balance
    #[inline(always)]
    pub fn order_margin(&self) -> M {
        self.order_margin
    }

    /// Return the available balance of account, same denotation as
    /// wallet_balance
    #[inline(always)]
    pub fn available_balance(&self) -> M {
        self.available_balance
    }

    /// Set a new order margin
    pub(crate) fn set_order_margin(&mut self, om: M) {
        trace!("set_order_margin: om: {}, self: {:?}", om, self);

        debug_assert!(om >= M::new_zero());

        self.order_margin = om;
        self.available_balance = self.wallet_balance - self.position_margin - self.order_margin;

        debug_assert!(self.available_balance >= M::new_zero());
    }

    /// Set the position margin by a given delta and adjust available balance
    /// accordingly
    pub(crate) fn set_position_margin(&mut self, val: M) {
        trace!("set_position_margin({}), self: {:?}", val, self);

        debug_assert!(val >= M::new_zero());

        self.position_margin = val;
        self.available_balance = self.wallet_balance - self.order_margin - self.position_margin;

        debug_assert!(self.position_margin >= M::new_zero());
        debug_assert!(self.position_margin <= self.wallet_balance);
        debug_assert!(self.available_balance >= M::new_zero());
        debug_assert!(self.available_balance <= self.wallet_balance);
    }

    /// Change the balance by a given delta e.g. from realizing profit or loss
    pub(crate) fn change_balance(&mut self, delta: M) {
        debug!("change_balance: delta: {}, self: {:?}", delta, self);

        self.wallet_balance += delta;
        self.available_balance += delta;

        debug_assert!(self.wallet_balance >= M::new_zero());
        debug_assert!(self.available_balance >= M::new_zero());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn margin_set_order_margin() {
        let mut margin = Margin::new_init(base!(1.0));
        margin.set_order_margin(base!(1.0));
        assert_eq!(margin.wallet_balance, base!(1.0));
        assert_eq!(margin.position_margin, base!(0.0));
        assert_eq!(margin.order_margin, base!(1.0));
        assert_eq!(margin.available_balance, base!(0.0));
    }

    #[test]
    #[should_panic]
    fn margin_set_order_margin_panic_0() {
        let mut margin = Margin::new_init(base!(1.0));
        margin.set_order_margin(base!(1.01));
    }

    #[test]
    #[should_panic]
    fn margin_set_order_margin_panic_1() {
        let mut margin = Margin::new_init(base!(1.0));
        margin.set_order_margin(base!(-0.1));
    }

    #[test]
    fn margin_set_position_margin() {
        // TODO:
    }

    #[test]
    fn margin_change_balance() {
        let mut margin = Margin::new_init(base!(1.0));

        margin.change_balance(base!(0.05));
        assert_eq!(margin.wallet_balance, base!(1.05));
        assert_eq!(margin.position_margin, base!(0.0));
        assert_eq!(margin.order_margin, base!(0.0));
        assert_eq!(margin.available_balance, base!(1.05));

        margin.change_balance(base!(-0.1));
        assert_eq!(margin.wallet_balance, base!(0.95));
        assert_eq!(margin.position_margin, base!(0.0));
        assert_eq!(margin.order_margin, base!(0.0));
        assert_eq!(margin.available_balance, base!(0.95));
    }

    #[test]
    #[should_panic]
    fn margin_change_balance_panic_0() {
        let mut margin = Margin::new_init(base!(1.0));

        margin.change_balance(base!(-1.01));
    }
}
