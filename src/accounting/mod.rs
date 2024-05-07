mod account;
mod in_memory_accounting;
mod transaction;
mod utils;

pub use in_memory_accounting::*;
pub use transaction::Transaction;

use crate::{types::MarginCurrency, Result};

/// The trait for settling transactions.
pub(crate) trait TransactionAccounting<M>
where
    M: MarginCurrency,
{
    /// Create a new instance with an initial balance for the user account.
    fn new(user_starting_wallet_balance: M) -> Self;

    /// Transfers a margin balance from one account to another.
    fn create_margin_transfer(&mut self, transaction: Transaction<M>) -> Result<()>;

    /// Query a balance of an account denoted in the margin currency.
    fn margin_balance_of(&self, account: AccountId) -> Result<M>;
}

/// The identifier of an account in the accounting infrastructure.
pub(crate) type AccountId = usize;
