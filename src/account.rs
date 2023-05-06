use crate::{
    position::Position,
    types::{Currency, MarginCurrency},
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
}

impl<M> Account<M>
where
    M: Currency + MarginCurrency,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: M) -> Self {
        let position = Position::default();

        Self {
            wallet_balance: starting_balance,
            position,
        }
    }

    /// Return a reference to the accounts position.
    #[inline(always)]
    pub fn position(&self) -> &Position<M> {
        &self.position
    }
}
