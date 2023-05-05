use crate::{
    position::Position,
    types::{Currency, MarginCurrency},
};

#[derive(Debug, Clone)]
/// The users account
/// Generic over:
/// S: The `Currency` representing the order quantity
pub struct Account<S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    pub(crate) wallet_balance: S::PairedCurrency,
    pub(crate) position: Position<S::PairedCurrency>,
}

impl<S> Account<S>
where
    S: Currency + Default,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: S::PairedCurrency) -> Self {
        let position = Position::default();

        Self {
            wallet_balance: starting_balance,
            position,
        }
    }

    /// Set a new position manually, be sure that you know what you are doing
    #[inline(always)]
    pub fn set_position(&mut self, position: Position<S::PairedCurrency>) {
        self.position = position;
    }

    /// Return a reference to the accounts position.
    #[inline(always)]
    pub fn position(&self) -> &Position<S::PairedCurrency> {
        &self.position
    }
}
