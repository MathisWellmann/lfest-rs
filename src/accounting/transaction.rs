use getset::CopyGetters;

use super::{
    AccountId, BROKER_MARGIN_ACCOUNT, EXCHANGE_FEE_ACCOUNT, MarginCurrency, Mon, QuoteCurrency,
    TREASURY_ACCOUNT, USER_ORDER_MARGIN_ACCOUNT, USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
};

/// A transaction involves two parties.
#[derive(Clone, CopyGetters, Debug)]
pub struct Transaction<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    #[getset(get_copy = "pub(crate)")]
    debit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    credit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    amount: BaseOrQuote,
    _quote: std::marker::PhantomData<QuoteCurrency<I, D>>,
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for Transaction<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Transaction( debit_account_id: {}, credit_account_id: {}, amount: {} )",
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

impl<I, const D: u8, BaseOrQuote> Transaction<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    pub(crate) fn new(
        debit_account_id: AccountId,
        credit_account_id: AccountId,
        amount: BaseOrQuote,
    ) -> Self {
        assert2::assert!(
            amount > BaseOrQuote::zero(),
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
            _quote: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::BaseCurrency;

    #[test]
    fn transaction_display() {
        let t = Transaction::new(0, 1, BaseCurrency::<i64, 1>::new(5, 0));
        assert_eq!(
            &t.to_string(),
            "Transaction( debit_account_id: USER_WALLET_ACCOUNT, credit_account_id: USER_ORDER_MARGIN_ACCOUNT, amount: 5.0 Base )"
        );
    }
}
