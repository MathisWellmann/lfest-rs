//! A clearinghouse clears and settles all trades and collects margin

use fpdec::{Dec, Decimal};

use crate::{
    account_tracker::NoAccountTracker,
    leverage,
    prelude::{Account, AccountTracker},
    types::{Currency, Leverage, MarginCurrency, QuoteCurrency},
};

/// A clearing house acts as an intermediary in futures transactions.
/// It guarantees the performance of the parties to each transaction.
/// The main task of the clearing house is to keep track of all the transactions
/// that take place, so that at can calculate the net position of each account.
///
/// If in total the transactions have lost money,
/// the account is required to provide variation margin to the exchange clearing
/// house. If there has been a gain on the transactions, the account receives
/// variation margin from the clearing house.
#[derive(Debug, Clone)]
pub struct ClearingHouse<A, S>
where
    S: Currency,
{
    /// The actual user of the exchange
    user_account: Account<A, S>,
    /// Just used to have an infinitely liquid counterparty for every transaction.
    counterparty: Account<NoAccountTracker, S>,
}

impl<A, S> ClearingHouse<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new instance with a user account
    pub(crate) fn new(account: Account<A, S>) -> Self {
        Self {
            user_account: account,
            counterparty: Account::new(
                NoAccountTracker::default(),
                leverage!(1),
                account.margin().wallet_balance() * Dec!(100),
            ),
        }
    }

    /// The margin accounts are adjusted to reflect investors gain or loss.
    pub(crate) fn mark_to_market(&mut self, mark_price: QuoteCurrency) {
        // let position_value = self.user_account.position().size().convert(mark_price);

        todo!()
    }

    /// The funding period for perpetual futures has ended.
    /// Funding = `mark_value` * `funding_rate`.
    /// `mark_value` is denoted in the margin currency.
    /// If the funding rate is positive, longs pay shorts.
    /// Else its the otherway around.
    pub(crate) fn settle_funding_period(
        &mut self,
        mark_value: S::PairedCurrency,
        funding_rate: Decimal,
    ) {
        todo!()
    }

    /// Get a reference to the user account
    #[inline(always)]
    pub(crate) fn user_account(&self) -> &Account<A, S> {
        &self.user_account
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_to_market() {
        todo!()
    }
}
