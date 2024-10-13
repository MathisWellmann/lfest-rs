use std::marker::PhantomData;

use getset::CopyGetters;

use super::{Mon, QuoteCurrency};
use crate::prelude::CurrencyMarker;

/// A T-Account keeps track of debits and credits.
#[derive(Debug, Default, Clone, Copy, CopyGetters)]
pub struct TAccount<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    #[getset(get_copy = "pub(crate)")]
    debits_posted: BaseOrQuote,
    #[getset(get_copy = "pub(crate)")]
    credits_posted: BaseOrQuote,
    _quote: PhantomData<QuoteCurrency<I, DB, DQ>>,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> TAccount<I, DB, DQ, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    pub(crate) fn post_debit(&mut self, amount: BaseOrQuote) {
        self.debits_posted += amount;
    }

    pub(crate) fn post_credit(&mut self, amount: BaseOrQuote) {
        self.credits_posted += amount;
    }

    pub(crate) fn net_balance(&self) -> BaseOrQuote {
        self.debits_posted - self.credits_posted
    }
}
