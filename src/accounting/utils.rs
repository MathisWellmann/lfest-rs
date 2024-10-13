use super::{account::TAccount, Mon};
use crate::types::MarginCurrencyMarker;

/// Asserts that the accounting equation holds true.
///
/// # Panics:
/// If the cumulative debits of all accounts don't equal credits.
pub(crate) fn debug_assert_accounting_equation<I, const DB: u8, const DQ: u8, BaseOrQuote>(
    accounts: &[TAccount<I, DB, DQ, BaseOrQuote>],
) where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: MarginCurrencyMarker<I, DB, DQ>,
{
    debug_assert!(
        {
            let mut debit_sum = BaseOrQuote::zero();
            let mut credit_sum = BaseOrQuote::zero();
            for account in accounts {
                debit_sum += account.debits_posted();
                credit_sum += account.credits_posted();
            }
            debit_sum == credit_sum
        },
        "The accounting balance has been violated"
    );
}
