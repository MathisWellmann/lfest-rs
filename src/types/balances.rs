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
    /// The available wallet balance that is used to provide margin for positions and orders.
    #[getset(get_copy = "pub")]
    available: BaseOrQuote,

    /// The margin reserved for the position.
    #[getset(get_copy = "pub")]
    position_margin: BaseOrQuote,

    /// The margin reserved for the open limit orders.
    #[getset(get_copy = "pub")]
    order_margin: BaseOrQuote,

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
            "available: {}, position_margin: {}, order_margin: {}",
            self.available, self.position_margin, self.order_margin
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
            available: init_balance,
            position_margin: BaseOrQuote::zero(),
            order_margin: BaseOrQuote::zero(),
            total_fees_paid: BaseOrQuote::zero(),
            _i: PhantomData,
        }
    }

    /// Sum of all balances.
    #[inline(always)]
    pub fn sum(&self) -> BaseOrQuote {
        self.available + self.position_margin + self.order_margin
    }

    pub(crate) fn debug_assert_state(&self) {
        assert2::debug_assert!(self.available >= BaseOrQuote::zero());
        assert2::debug_assert!(self.position_margin >= BaseOrQuote::zero());
        assert2::debug_assert!(self.order_margin >= BaseOrQuote::zero());
    }

    /// If `fee` is negative then we receive balance.
    #[inline(always)]
    pub fn account_for_fee(&mut self, fee: BaseOrQuote) {
        trace!("account_for_fee: {fee}");
        self.debug_assert_state();

        self.available -= fee;
        assert2::debug_assert!(self.available >= BaseOrQuote::zero());

        self.total_fees_paid += fee;
    }

    /// Try to reserve some order margin from available balance.
    #[inline]
    #[must_use]
    pub fn try_reserve_order_margin(&mut self, margin: BaseOrQuote) -> bool {
        trace!("try_reserve_order_margin {margin} on self: {self}");
        assert2::debug_assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();

        if margin > self.available {
            return false;
        }

        self.available -= margin;
        assert2::debug_assert!(self.available >= BaseOrQuote::zero());
        self.order_margin += margin;
        true
    }

    /// Cancelling an order requires freeing the locked margin balance.
    #[inline(always)]
    pub fn free_order_margin(&mut self, margin: BaseOrQuote) {
        trace!("free_order_margin: {margin} on self: {self}");
        assert2::debug_assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();
        assert2::debug_assert!(self.order_margin >= margin);

        self.order_margin -= margin;
        assert2::debug_assert!(self.order_margin >= BaseOrQuote::zero());
        self.available += margin;
    }

    /// Closing a position frees the position margin.
    #[inline(always)]
    pub fn free_position_margin(&mut self, margin: BaseOrQuote) {
        trace!("free_position_margin: {margin} on self: {self}");
        assert2::debug_assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();
        assert2::debug_assert!(self.position_margin >= margin);

        self.position_margin -= margin;
        assert2::debug_assert!(self.position_margin >= BaseOrQuote::zero());
        self.available += margin;
    }

    /// Try to reserve some position margin from available balance.
    #[inline(always)]
    pub fn try_reserve_position_margin(&mut self, margin: BaseOrQuote) -> bool {
        trace!("try_reserve_position_margin {margin} on self: {self}");
        assert2::debug_assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();

        if margin > self.available {
            return false;
        }

        self.available -= margin;
        assert2::debug_assert!(self.available >= BaseOrQuote::zero());
        self.position_margin += margin;
        true
    }

    /// Profit and loss are applied to the available balance.
    #[inline(always)]
    pub fn apply_pnl(&mut self, pnl: BaseOrQuote) {
        trace!("apply_pnl: {pnl}, self: {self}");
        self.available += pnl;
        assert2::debug_assert!(self.available >= BaseOrQuote::zero());
    }
}

#[cfg(test)]
mod test {
    use num::Zero;
    use proptest::prelude::*;

    use super::*;
    use crate::types::QuoteCurrency;

    #[test]
    fn user_balances() {
        let balances = Balances {
            available: QuoteCurrency::<i64, 5>::new(1000, 0),
            position_margin: QuoteCurrency::new(200, 0),
            order_margin: QuoteCurrency::new(100, 0),
            total_fees_paid: QuoteCurrency::zero(),
            _i: PhantomData,
        };
        assert_eq!(balances.sum(), QuoteCurrency::new(1300, 0));
    }

    proptest! {
        #[test]
        fn proptest_balances_account_for_fee(fee in 0..1000_i64) {
            let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(10_000, 0));
            let fee = QuoteCurrency::new(fee, 0);
            balances.account_for_fee(fee);
            balances.debug_assert_state();
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - fee)
                .position_margin(Zero::zero())
                .order_margin(Zero::zero())
                .total_fees_paid(fee)
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn proptest_balances_try_reserve_order_margin(margin in 1..1000_i64) {
            let margin = QuoteCurrency::new(margin, 0);
            let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
            assert!(balances.try_reserve_order_margin(margin));
            balances.debug_assert_state();
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(1000, 0) - margin)
                .position_margin(Zero::zero())
                .order_margin(margin)
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn proptest_balances_free_order_margin(margin in 1..1000_i64) {
            let margin = QuoteCurrency::new(margin, 0);
            let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
            assert!(balances.try_reserve_order_margin(margin));
            balances.free_order_margin(margin);
            balances.debug_assert_state();
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(1000, 0))
                .position_margin(Zero::zero())
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    #[test]
    #[should_panic]
    fn balances_free_order_margin_panic() {
        let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
        balances.free_order_margin(QuoteCurrency::new(100, 0));
    }

    proptest! {
        #[test]
        fn proptest_balances_try_reserve_position_margin(margin in 1..1000_i64) {
            let margin = QuoteCurrency::new(margin, 0);
            let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
            assert!(balances.try_reserve_position_margin(margin));
            balances.debug_assert_state();
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(1000, 0) - margin)
                .position_margin(margin)
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    #[test]
    fn balances_try_reserve_position_margin_false() {
        let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
        assert!(!balances.try_reserve_position_margin(QuoteCurrency::new(1001, 0)));
    }

    proptest! {
        #[test]
        fn proptest_balances_free_position_margin(margin in 1..1000_i64) {
            let margin = QuoteCurrency::new(margin, 0);
            let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
            assert!(balances.try_reserve_position_margin(margin));
            balances.free_position_margin(margin);
            balances.debug_assert_state();
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(1000, 0))
                .position_margin(Zero::zero())
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    #[test]
    #[should_panic]
    fn balances_free_position_margin_panic() {
        let mut balances = Balances::new(QuoteCurrency::<i64, 5>::new(1000, 0));
        balances.free_position_margin(QuoteCurrency::new(100, 0));
    }

    #[test]
    #[should_panic]
    fn balances_debug_assert_state_available() {
        let balances = Balances::builder()
            .available(QuoteCurrency::<i64, 5>::new(-1, 0))
            .position_margin(Zero::zero())
            .order_margin(Zero::zero())
            .total_fees_paid(Zero::zero())
            .build();
        balances.debug_assert_state();
    }

    #[test]
    #[should_panic]
    fn balances_debug_assert_state_position_margin() {
        let balances = Balances::builder()
            .available(Zero::zero())
            .position_margin(QuoteCurrency::<i64, 5>::new(-1, 0))
            .order_margin(Zero::zero())
            .total_fees_paid(Zero::zero())
            .build();
        balances.debug_assert_state();
    }

    #[test]
    #[should_panic]
    fn balances_debug_assert_state_order_margin() {
        let balances = Balances::builder()
            .available(Zero::zero())
            .position_margin(Zero::zero())
            .order_margin(QuoteCurrency::<i64, 5>::new(-1, 0))
            .total_fees_paid(Zero::zero())
            .build();
        balances.debug_assert_state();
    }

    #[test]
    fn balances_display() {
        let balances = Balances::builder()
            .available(QuoteCurrency::<i64, 5>::new(1000, 0))
            .position_margin(QuoteCurrency::new(50, 0))
            .order_margin(QuoteCurrency::new(100, 0))
            .total_fees_paid(QuoteCurrency::new(5, 0))
            .build();
        assert_eq!(
            balances.to_string(),
            "available: 1000.00000 Quote, position_margin: 50.00000 Quote, order_margin: 100.00000 Quote"
        );
    }
}
