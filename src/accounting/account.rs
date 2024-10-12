use getset::CopyGetters;

use crate::prelude::{CurrencyMarker, Mon, Monies};

/// A T-Account keeps track of debits and credits.
#[derive(Debug, Default, Clone, Copy, CopyGetters)]
pub struct TAccount<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    #[getset(get_copy = "pub(crate)")]
    debits_posted: Monies<T, BaseOrQuote>,
    #[getset(get_copy = "pub(crate)")]
    credits_posted: Monies<T, BaseOrQuote>,
}

impl<T, BaseOrQuote> TAccount<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    pub(crate) fn post_debit(&mut self, amount: Monies<T, BaseOrQuote>) {
        self.debits_posted += amount;
    }

    pub(crate) fn post_credit(&mut self, amount: Monies<T, BaseOrQuote>) {
        self.credits_posted += amount;
    }

    pub(crate) fn net_balance(&self) -> Monies<T, BaseOrQuote> {
        self.debits_posted - self.credits_posted
    }
}
