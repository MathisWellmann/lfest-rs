use super::{Mon, account::TAccount};
use crate::types::MarginCurrency;

/// Asserts that the accounting equation holds true.
///
/// # Panics:
/// If the cumulative debits of all accounts don't equal credits.
pub(crate) fn debug_assert_accounting_equation<I, const D: u8, BaseOrQuote>(
    accounts: &[TAccount<I, D, BaseOrQuote>],
) where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::BaseCurrency;

    #[test]
    fn test_accounting_equation() {
        debug_assert_accounting_equation(&[TAccount::<i64, 1, BaseCurrency<_, 1>>::default(); 5]);
        debug_assert_accounting_equation(&[TAccount::<i32, 1, BaseCurrency<_, 1>>::default(); 5]);
    }

    #[test]
    #[should_panic]
    fn test_accounting_equation_panic() {
        let mut accounts = [TAccount::default()];
        accounts[0].post_credit(BaseCurrency::<i64, 1>::new(5, 0));
        debug_assert_accounting_equation(&accounts);
    }
}
