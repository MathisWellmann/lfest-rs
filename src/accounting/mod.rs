mod account;
mod in_memory_accounting;
mod transaction;
mod utils;

#[cfg(test)]
pub(crate) use account::TAccount;
pub use in_memory_accounting::*;
pub use transaction::Transaction;

use crate::prelude::*;

/// The trait for settling transactions.
pub trait TransactionAccounting<I, const D: u8, BaseOrQuote>: std::fmt::Debug
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    /// Create a new instance with an initial balance for the user account.
    fn new(user_starting_wallet_balance: BaseOrQuote) -> Self;

    /// Transfers a margin balance from one account to another.
    fn create_margin_transfer(&mut self, transaction: Transaction<I, D, BaseOrQuote>)
    -> Result<()>;

    /// Query a balance of an account denoted in the margin currency.
    fn margin_balance_of(&self, account: AccountId) -> Result<BaseOrQuote>;
}

/// The identifier of an account in the accounting infrastructure.
pub(crate) type AccountId = usize;

// No-Op implementation which can be useful sometimes.
impl<I, const D: u8, BaseOrQuote> TransactionAccounting<I, D, BaseOrQuote> for ()
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    fn new(_user_starting_wallet_balance: BaseOrQuote) -> Self {}

    fn create_margin_transfer(
        &mut self,
        _transaction: Transaction<I, D, BaseOrQuote>,
    ) -> Result<()> {
        Ok(())
    }

    fn margin_balance_of(&self, _account: AccountId) -> Result<BaseOrQuote> {
        Ok(BaseOrQuote::zero())
    }
}
