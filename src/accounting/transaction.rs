use getset::CopyGetters;

use super::{
    AccountId, MarginCurrencyMarker, Mon, QuoteCurrency, BROKER_MARGIN_ACCOUNT,
    EXCHANGE_FEE_ACCOUNT, TREASURY_ACCOUNT, USER_ORDER_MARGIN_ACCOUNT,
    USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
};

/// A transaction involves two parties.
#[derive(Clone, CopyGetters)]
pub struct Transaction<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
{
    #[getset(get_copy = "pub(crate)")]
    debit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    credit_account_id: AccountId,
    #[getset(get_copy = "pub(crate)")]
    amount: BaseOrQuote,
    _quote: std::marker::PhantomData<QuoteCurrency<I, D>>,
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Debug for Transaction<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
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

impl<I, const D: u8, BaseOrQuote> Transaction<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
{
    pub(crate) fn new(
        debit_account_id: AccountId,
        credit_account_id: AccountId,
        amount: BaseOrQuote,
    ) -> Self {
        assert!(
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
