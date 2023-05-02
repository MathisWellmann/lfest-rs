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
}

impl<M> Margin<M>
where
    M: Currency,
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
        }
    }

    /// Create a new Margin with all custom fields.
    #[inline]
    pub fn new(wallet_balance: M, position_margin: M, order_margin: M) -> Result<Self> {
        if position_margin > wallet_balance {
            return Err(Error::InvalidPositionMargin);
        }
        if order_margin > wallet_balance {
            return Err(Error::InvalidOrderMargin);
        }

        Ok(Margin {
            wallet_balance,
            position_margin,
            order_margin,
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
        self.wallet_balance - self.order_margin - self.position_margin
    }

    /// Locks some balance as order collateral.
    pub(crate) fn lock_as_order_collateral(&mut self, amount: M) -> Result<()> {
        let ab = self.available_balance();
        if amount > ab {
            return Err(Error::NotEnoughAvailableBalance);
        }

        self.order_margin += amount;
        Ok(())
    }

    /// Locks some balance as position collateral.
    pub(crate) fn lock_as_position_collateral(&mut self, amount: M) -> Result<()> {
        let ab = self.available_balance();
        if amount > ab {
            return Err(Error::NotEnoughAvailableBalance);
        }

        self.position_margin += amount;
        Ok(())
    }

    /// Entirely frees the order margin.
    #[inline(always)]
    pub(crate) fn clear_order_margin(&mut self) {
        self.order_margin = M::new_zero();
    }

    /// Entirely frees the position margin.
    #[inline(always)]
    pub(crate) fn clear_position_margin(&mut self) {
        self.position_margin = M::new_zero();
    }

    /// Unlocks a specific `amount` of order margin.
    pub(crate) fn unlock_order_margin(&mut self, amount: M) -> Result<()> {
        if amount > self.order_margin {
            return Err(Error::NotEnoughOrderMargin);
        }
        self.order_margin -= amount;

        Ok(())
    }

    /// Unlocks a specific `amount` of position margin.
    pub(crate) fn free_position_margin(&mut self, amount: M) -> Result<()> {
        if amount > self.position_margin {
            return Err(Error::NotEnoughOrderMargin);
        }
        self.position_margin -= amount;

        Ok(())
    }
}
