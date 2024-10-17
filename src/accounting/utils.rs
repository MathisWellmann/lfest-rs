use super::{account::TAccount, Mon};
use crate::types::MarginCurrencyMarker;

/// Asserts that the accounting equation holds true.
///
/// # Panics:
/// If the cumulative debits of all accounts don't equal credits.
pub(crate) fn debug_assert_accounting_equation<I, const D: u8, BaseOrQuote>(
    accounts: &[TAccount<I, D, BaseOrQuote>],
) where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
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
