use num_traits::Zero;

use super::account::TAccount;
use crate::{
    prelude::{Mon, Monies},
    types::MarginCurrencyMarker,
};

/// Asserts that the accounting equation holds true.
///
/// # Panics:
/// If the cumulative debits of all accounts don't equal credits.
pub(crate) fn debug_assert_accounting_equation<T, BaseOrQuote>(
    accounts: &[TAccount<T, BaseOrQuote>],
) where
    T: Mon,
    BaseOrQuote: MarginCurrencyMarker<T>,
{
    debug_assert!(
        {
            let mut debit_sum = Monies::zero();
            let mut credit_sum = Monies::zero();
            for account in accounts {
                debit_sum += account.debits_posted();
                credit_sum += account.credits_posted();
            }
            debit_sum == credit_sum
        },
        "The accounting balance has been violated"
    );
}
