//! A clearinghouse clears and settles all trades and collects margin

use fpdec::Decimal;

use crate::{
    prelude::{Account, AccountTracker},
    risk_engine::RiskEngine,
    types::{Currency, MarginCurrency, QuoteCurrency},
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
pub struct ClearingHouse<A, S, R>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    /// Manages the risk if positions.
    risk_engine: R,
    /// Keeps track of all trades of the `Account`.
    account_tracker: A,
    /// The actual user of the exchange
    user_account: Account<S>,
}

impl<A, S, R> ClearingHouse<A, S, R>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
    S::PairedCurrency: MarginCurrency,
    R: RiskEngine<S::PairedCurrency>,
{
    /// Create a new instance with a user account
    pub(crate) fn new(risk_engine: R, account_tracker: A, user_account: Account<S>) -> Self {
        Self {
            risk_engine,
            account_tracker,
            user_account,
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
    pub(crate) fn user_account(&self) -> &Account<S> {
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
