use crate::{
    position::Position,
    types::{Currency, Leverage, MarginCurrency, Result},
};

#[derive(Debug, Clone)]
/// The users account
/// Generic over:
/// S: The `Currency` representing the order quantity
pub struct Account<M>
where
    M: Currency + MarginCurrency,
{
    pub(crate) wallet_balance: M,
    pub(crate) position: Position<M>,
    /// Because the `Account` only holds 1 `Position`, the `desired_leverage` is stored here,
    /// but its closely coupled with the value and margin of the position.
    pub(crate) desired_leverage: Leverage,
}

impl<M> Account<M>
where
    M: Currency + MarginCurrency,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: M, desired_leverage: Leverage) -> Self {
        let position = Position::default();

        Self {
            wallet_balance: starting_balance,
            position,
            desired_leverage,
        }
    }

    /// Return a reference to the accounts position.
    #[inline(always)]
    pub fn position(&self) -> &Position<M> {
        &self.position
    }

    /// Allows the user to update their desired leverage.
    /// This will deposit or release variation margin from the position if any.
    ///
    /// # Returns:
    /// If Err, the account is unable to provide enough variation margin for the desired leverage.
    pub fn update_desired_leverage(&mut self, leverage: Leverage) -> Result<()> {
        todo!()
    }
}
