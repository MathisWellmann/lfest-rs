use tracing::trace;

use super::{
    account::TAccount, transaction::Transaction, utils::debug_assert_accounting_equation,
    AccountId, Mon, TransactionAccounting,
};
use crate::{
    types::{Error, MarginCurrencyMarker},
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
#[derive(Debug)]
pub struct InMemoryTransactionAccounting<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: MarginCurrencyMarker<I, DB, DQ>,
{
    /// Accounts are allocated at the start as they are known upfront.
    margin_accounts: [TAccount<I, DB, DQ, BaseOrQuote>; N_ACCOUNTS],
    // TODO: keep track of transaction log or emit `Transactions` to users.
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> TransactionAccounting<I, DB, DQ, BaseOrQuote>
    for InMemoryTransactionAccounting<I, DB, DQ, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: MarginCurrencyMarker<I, DB, DQ>,
{
    fn new(user_starting_wallet_balance: BaseOrQuote) -> Self {
        let mut s = Self {
            margin_accounts: [TAccount::default(); N_ACCOUNTS],
        };
        s.margin_accounts[USER_WALLET_ACCOUNT].post_debit(user_starting_wallet_balance);
        s.margin_accounts[TREASURY_ACCOUNT].post_credit(user_starting_wallet_balance);
        debug_assert_accounting_equation(&s.margin_accounts);

        s
    }

    fn create_margin_transfer(
        &mut self,
        transaction: Transaction<I, DB, DQ, BaseOrQuote>,
    ) -> Result<(), I, DB, DQ> {
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

        debug_assert_accounting_equation(&self.margin_accounts);

        Ok(())
    }

    fn margin_balance_of(&self, account: AccountId) -> Result<BaseOrQuote, I, DB, DQ> {
        self.margin_accounts
            .get(account)
            .ok_or(Error::AccountLookupFailure)
            .map(|account| account.net_balance())
    }
}
