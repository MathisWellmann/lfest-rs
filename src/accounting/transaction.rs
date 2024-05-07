use getset::CopyGetters;

use super::AccountId;
use crate::types::Currency;

/// A transaction involves two parties.
#[derive(Debug, Clone, CopyGetters)]
pub struct Transaction<Q>
where
    Q: Currency,
{
    #[getset(get_copy = "pub(crate)")]
    debit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    credit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    amount: Q,
}

impl<Q> Transaction<Q>
where
    Q: Currency,
{
    pub(crate) fn new(
        debit_account_id: AccountId,
        credit_account_id: AccountId,
        amount: Q,
    ) -> Self {
        assert!(
            amount > Q::new_zero(),
            "The amount of a transaction must be greater than zero"
        );
        assert_ne!(
            debit_account_id, credit_account_id,
            "The debit and credit accounts must not be the same"
        );
        Self {
            debit_account_id,
            credit_account_id,
            amount,
        }
    }
}
