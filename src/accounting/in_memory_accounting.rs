use tracing::trace;

use super::{
    account::Account, transaction::Transaction, utils::assert_accounting_equation, AccountId,
    TransactionAccounting,
};
use crate::{
    types::{Error, MarginCurrency},
    Result,
};

const N_ACCOUNTS: usize = 6;
// The accounts denoted in the margin currency.
/// The users wallet account.
pub const USER_WALLET_ACCOUNT: usize = 0;
/// The users order margin account
pub const USER_ORDER_MARGIN_ACCOUNT: usize = 1;
/// The users position margin account.
pub const USER_POSITION_MARGIN_ACCOUNT: usize = 2;
/// The exchanges fee account.
pub const EXCHANGE_FEE_ACCOUNT: usize = 3;
/// The brokers margin account.
pub const BROKER_MARGIN_ACCOUNT: usize = 4;
/// The treasury account.
pub const TREASURY_ACCOUNT: usize = 5;

/// Keeps track of transaction in memory.
pub struct InMemoryTransactionAccounting<M>
where
    M: MarginCurrency,
{
    /// Accounts are allocated at the start as they are known upfront.
    margin_accounts: [Account<M>; N_ACCOUNTS],
    // TODO: keep track of transaction log or emit `Transactions` to users.
}

impl<M> TransactionAccounting<M> for InMemoryTransactionAccounting<M>
where
    M: MarginCurrency,
{
    fn new(user_starting_wallet_balance: M) -> Self {
        let mut s = Self {
            margin_accounts: [Account::default(); N_ACCOUNTS],
        };
        s.margin_accounts[USER_WALLET_ACCOUNT].post_debit(user_starting_wallet_balance);
        s.margin_accounts[TREASURY_ACCOUNT].post_credit(user_starting_wallet_balance);
        assert_accounting_equation(&s.margin_accounts);

        s
    }

    fn create_margin_transfer(&mut self, transaction: Transaction<M>) -> Result<()> {
        trace!("create_margin_transfer: {transaction:?}");
        let mut debit_account = self
            .margin_accounts
            .get(transaction.debit_account_id())
            .cloned()
            .ok_or(Error::AccountLookupFailure)?;
        let credit_account = self
            .margin_accounts
            .get_mut(transaction.credit_account_id())
            .ok_or(Error::AccountLookupFailure)?;

        let amnt = transaction.amount();
        debit_account.post_debit(amnt);
        credit_account.post_credit(amnt);

        self.margin_accounts[transaction.debit_account_id()] = debit_account;

        assert_accounting_equation(&self.margin_accounts);

        Ok(())
    }

    fn margin_balance_of(&self, account: AccountId) -> Result<M> {
        self.margin_accounts
            .get(account)
            .ok_or(Error::AccountLookupFailure)
            .map(|account| account.net_balance())
    }
}
