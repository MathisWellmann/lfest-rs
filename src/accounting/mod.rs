mod account;
mod in_memory_accounting;
mod transaction;
mod utils;

pub use in_memory_accounting::*;
pub use transaction::Transaction;

use crate::{
    prelude::{Mon, Monies},
    types::MarginCurrencyMarker,
    Result,
};

/// The trait for settling transactions.
pub trait TransactionAccounting<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: MarginCurrencyMarker<T>,
{
    /// Create a new instance with an initial balance for the user account.
    fn new(user_starting_wallet_balance: Monies<T, BaseOrQuote>) -> Self;

    /// Transfers a margin balance from one account to another.
    fn create_margin_transfer(&mut self, transaction: Transaction<T, BaseOrQuote>)
        -> Result<(), T>;

    /// Query a balance of an account denoted in the margin currency.
    fn margin_balance_of(&self, account: AccountId) -> Result<Monies<T, BaseOrQuote>, T>;
}

/// The identifier of an account in the accounting infrastructure.
pub(crate) type AccountId = usize;
