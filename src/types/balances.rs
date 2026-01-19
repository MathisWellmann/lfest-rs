use std::marker::PhantomData;

use getset::CopyGetters;
use tracing::trace;
use typed_builder::TypedBuilder;

use super::{
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
            "available: {}, total_fees_paid: {}",
            self.equity, self.total_fees_paid,
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
            _i: PhantomData,
        }
    }

    #[inline]
    pub(crate) fn debug_assert_state(&self) {
        assert2::debug_assert!(self.equity >= BaseOrQuote::zero());
    }

    /// If `fee` is negative then we receive balance.
    #[inline(always)]
    pub fn account_for_fee(&mut self, fee: BaseOrQuote) {
        trace!("account_for_fee: {fee}");
        self.debug_assert_state();

        self.equity -= fee;
        assert2::debug_assert!(self.equity >= BaseOrQuote::zero());

        self.total_fees_paid += fee;
    }

    /// Profit and loss are applied to the available balance.
    #[inline(always)]
    pub fn apply_pnl(&mut self, pnl: BaseOrQuote) {
        trace!("apply_pnl: {pnl}, self: {self}");
        self.equity += pnl;
        assert2::debug_assert!(self.equity >= BaseOrQuote::zero());
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
        assert_eq!(size_of::<Balances<i32, 5, BaseCurrency<i32, 5>>>(), 8);
        assert_eq!(size_of::<Balances<i64, 5, BaseCurrency<i64, 5>>>(), 16);
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
    fn balances_display() {
        let balances = Balances::builder()
            .equity(QuoteCurrency::<i64, 5>::new(1000, 0))
            .total_fees_paid(QuoteCurrency::new(5, 0))
            .build();
        assert_eq!(
            balances.to_string(),
            "available: 1000.00000 Quote, total_fees_paid: 5.00000 Quote"
        );
    }
}
