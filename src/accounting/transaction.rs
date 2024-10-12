use getset::CopyGetters;
use num_traits::Zero;

use super::{
    AccountId, BROKER_MARGIN_ACCOUNT, EXCHANGE_FEE_ACCOUNT, TREASURY_ACCOUNT,
    USER_ORDER_MARGIN_ACCOUNT, USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
};
use crate::prelude::{CurrencyMarker, Mon, Monies};

/// A transaction involves two parties.
#[derive(Clone, CopyGetters)]
pub struct Transaction<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    #[getset(get_copy = "pub(crate)")]
    debit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    credit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    amount: Monies<T, BaseOrQuote>,
}

impl<T, BaseOrQuote> std::fmt::Debug for Transaction<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "debit_account_id: {}, credit_account_id: {}, amount: {}",
            account_from_int(self.debit_account_id),
            account_from_int(self.credit_account_id),
            self.amount
        )
    }
}

/// For making accounts more readable in `Debug` formatting.
fn account_from_int(int: usize) -> &'static str {
    match int {
        USER_WALLET_ACCOUNT => "USER_WALLET_ACCOUNT",
        USER_ORDER_MARGIN_ACCOUNT => "USER_ORDER_MARGIN_ACCOUNT",
        USER_POSITION_MARGIN_ACCOUNT => "USER_POSITION_MARGIN_ACCOUNT",
        EXCHANGE_FEE_ACCOUNT => "EXCHANGE_FEE_ACCOUNT",
        BROKER_MARGIN_ACCOUNT => "BROKER_MARGIN_ACCOUNT",
        TREASURY_ACCOUNT => "TREASURY_ACCOUNT",
        _ => panic!("invalid account"),
    }
}

impl<T, BaseOrQuote> Transaction<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    pub(crate) fn new(
        debit_account_id: AccountId,
        credit_account_id: AccountId,
        amount: Monies<T, BaseOrQuote>,
    ) -> Self {
        assert!(
            amount > Monies::zero(),
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
