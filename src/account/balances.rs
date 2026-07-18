use std::marker::PhantomData;

use getset::CopyGetters;
use tracing::trace;
use typed_builder::TypedBuilder;

use crate::types::{
    MarginCurrency,
    Mon,
};

/// Contains user balances including margin amounts.
#[derive(Debug, Clone, Eq, PartialEq, TypedBuilder, CopyGetters)]
pub struct Balances<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    /// The wallet balance that is used to provide margin for positions and orders.
    #[getset(get_copy = "pub")]
    equity: BaseOrQuote,

    // TODO: could be removed here and done differently.
    /// The total amount of fees paid or received.
    #[getset(get_copy = "pub")]
    total_fees_paid: BaseOrQuote,

    /// The cumulative losses which exceeded the account equity and were absorbed by the
    /// venue (insurance fund / auto-deleveraging on a real exchange).
    /// Non-zero bad debt means the account went bankrupt; its equity is floored at zero.
    #[getset(get_copy = "pub")]
    #[builder(default)]
    bad_debt: BaseOrQuote,

    /// A marker type.
    #[builder(default)]
    _i: PhantomData<I>,
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for Balances<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "equity: {}, total_fees_paid: {}, bad_debt: {}",
            self.equity, self.total_fees_paid, self.bad_debt,
        )
    }
}

impl<I, const D: u8, BaseOrQuote> Balances<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    /// Create a new instance with an initial balance.
    pub fn new(init_balance: BaseOrQuote) -> Self {
        Self {
            equity: init_balance,
            total_fees_paid: BaseOrQuote::zero(),
            bad_debt: BaseOrQuote::zero(),
            _i: PhantomData,
        }
    }

    #[inline(always)]
    pub(crate) fn debug_assert_state(&self) {
        assert2::debug_assert!(self.equity >= BaseOrQuote::zero());
    }

    /// If `fee` is negative then we receive balance.
    /// A fee exceeding the equity bankrupts the account; see `Balances::apply_to_equity`.
    #[inline(always)]
    pub fn account_for_fee(&mut self, fee: BaseOrQuote) {
        self.debug_assert_state();

        self.apply_to_equity(-fee);
        self.total_fees_paid += fee;
    }

    /// Profit and loss are applied to the available balance.
    /// A loss exceeding the equity bankrupts the account; see `Balances::apply_to_equity`.
    #[inline(always)]
    pub fn apply_pnl(&mut self, pnl: BaseOrQuote) {
        trace!("apply_pnl: {pnl}");
        self.apply_to_equity(pnl);
    }

    /// Apply a signed equity change from a realized pnl or fee.
    ///
    /// A change which would push the equity below zero bankrupts the account:
    /// a real venue absorbs the excess loss (insurance fund / auto-deleveraging)
    /// rather than collecting it from the trader, so the excess is recorded as
    /// [`Balances::bad_debt`] and the equity is floored at zero.
    #[inline(always)]
    fn apply_to_equity(&mut self, delta: BaseOrQuote) {
        let new_equity = self.equity + delta;
        if new_equity < BaseOrQuote::zero() {
            core::hint::cold_path();
            tracing::warn!(
                "account is bankrupt: the venue absorbs {} of bad debt",
                -new_equity
            );
            self.bad_debt -= new_equity;
            self.equity = BaseOrQuote::zero();
        } else {
            self.equity = new_equity;
        }
    }
}

#[cfg(test)]
mod test {
    use num::Zero;
    use proptest::prelude::*;

    use super::*;
    use crate::types::{
        BaseCurrency,
        QuoteCurrency,
    };

    #[test]
    fn size_of_balances() {
        assert_eq!(size_of::<Balances<i32, 5, BaseCurrency<i32, 5>>>(), 12);
        assert_eq!(size_of::<Balances<i64, 5, BaseCurrency<i64, 5>>>(), 24);
    }

    proptest! {
        #[test]
        fn proptest_balances_account_for_fee(fee in 0..1000_i64) {
            let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(10_000, 0));
            let fee = QuoteCurrency::new(fee, 0);
            balances.account_for_fee(fee);
            balances.debug_assert_state();
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0) - fee)
                .total_fees_paid(fee)
                .build()
            );
            assert!(balances.bad_debt().is_zero());
        }
    }

    #[test]
    #[should_panic]
    fn balances_debug_assert_state_available() {
        let balances = Balances::builder()
            .equity(QuoteCurrency::<i64, 5>::new(-1, 0))
            .total_fees_paid(Zero::zero())
            .build();
        balances.debug_assert_state();
    }

    #[test]
    fn bankrupting_loss_is_recorded_as_bad_debt() {
        let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(100, 0));
        balances.apply_pnl(QuoteCurrency::new(-150, 0));
        assert!(balances.equity().is_zero());
        assert_eq!(balances.bad_debt(), QuoteCurrency::new(50, 0));
        balances.debug_assert_state();

        let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(100, 0));
        balances.account_for_fee(QuoteCurrency::new(101, 0));
        assert!(balances.equity().is_zero());
        assert_eq!(balances.bad_debt(), QuoteCurrency::new(1, 0));
        assert_eq!(balances.total_fees_paid(), QuoteCurrency::new(101, 0));
    }

    #[test]
    fn balances_display() {
        let balances = Balances::builder()
            .equity(QuoteCurrency::<i64, 5>::new(1000, 0))
            .total_fees_paid(QuoteCurrency::new(5, 0))
            .build();
        assert_eq!(
            balances.to_string(),
            "equity: 1000.00000 Quote, total_fees_paid: 5.00000 Quote, bad_debt: 0.00000 Quote"
        );
    }
}
