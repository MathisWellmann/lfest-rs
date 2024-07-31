use super::account::TAccount;
use crate::types::MarginCurrency;

/// Asserts that the accounting equation holds true.
///
/// # Panics:
/// If the cumulative debits of all accounts don't equal credits.
pub(crate) fn debug_assert_accounting_equation<M>(accounts: &[TAccount<M>])
where
    M: MarginCurrency,
{
    debug_assert!(
        {
            let mut debit_sum = M::new_zero();
            let mut credit_sum = M::new_zero();
            for account in accounts {
                debit_sum += account.debits_posted();
                credit_sum += account.credits_posted();
            }
            debit_sum == credit_sum
        },
        "The accounting balance has been violated"
    );
}
