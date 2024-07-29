use getset::CopyGetters;

use crate::types::Currency;

/// A T-Account keeps track of debits and credits.
#[derive(Debug, Default, Clone, Copy, CopyGetters)]
pub struct TAccount<Q>
where
    Q: Currency,
{
    #[getset(get_copy = "pub(crate)")]
    debits_posted: Q,
    #[getset(get_copy = "pub(crate)")]
    credits_posted: Q,
}

impl<Q> TAccount<Q>
where
    Q: Currency,
{
    pub(crate) fn post_debit(&mut self, amount: Q) {
        self.debits_posted += amount;
    }

    pub(crate) fn post_credit(&mut self, amount: Q) {
        self.credits_posted += amount;
    }

    pub(crate) fn net_balance(&self) -> Q {
        self.debits_posted - self.credits_posted
    }

    #[cfg(test)]
    pub(crate) fn from_parts(debits_posted: Q, credits_posted: Q) -> Self {
        Self {
            debits_posted,
            credits_posted,
        }
    }
}
