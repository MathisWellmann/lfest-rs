use std::marker::PhantomData;

use getset::CopyGetters;
use tracing::trace;
use typed_builder::TypedBuilder;

use super::{MarginCurrency, Mon};

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
    #[inline]
    pub fn sum(&self) -> BaseOrQuote {
        self.available + self.position_margin + self.order_margin
    }

    pub(crate) fn debug_assert_state(&self) {
        assert2::debug_assert!(self.available >= BaseOrQuote::zero());
        assert2::debug_assert!(self.position_margin >= BaseOrQuote::zero());
        assert2::debug_assert!(self.order_margin >= BaseOrQuote::zero());
    }

    /// If `fee` is negative then we receive balance.
    #[inline]
    pub fn account_for_fee(&mut self, fee: BaseOrQuote) {
        self.debug_assert_state();

        self.available -= fee;
        self.total_fees_paid += fee;
    }

    /// Placing an order requires locking margin balance.
    #[inline]
    #[must_use]
    pub fn try_reserve_order_margin(&mut self, init_margin: BaseOrQuote) -> bool {
        trace!("try_reserve_order_margin {init_margin} on self: {self}");
        assert2::assert!(init_margin > BaseOrQuote::zero());
        self.debug_assert_state();

        if init_margin > self.available {
            return false;
        }

        self.available = self.available - init_margin;
        self.order_margin = self.order_margin + init_margin;
        true
    }

    /// Filling an order requires moving order margin to position margin.
    #[inline]
    pub fn fill_order(&mut self, margin: BaseOrQuote) {
        assert2::assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();
        assert2::assert!(self.order_margin >= margin);

        self.order_margin -= margin;
        self.position_margin += margin;
    }

    /// Cancelling an order requires freeing the locked margin balance.
    #[inline]
    pub fn free_order_margin(&mut self, margin: BaseOrQuote) {
        trace!("free_order_margin: {margin} on self: {self}");
        assert2::assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();
        assert2::assert!(self.order_margin >= margin);

        self.order_margin -= margin;
        self.available += margin;
    }

    /// Closing a position frees the position margin.
    #[inline]
    pub fn free_position_margin(&mut self, margin: BaseOrQuote) {
        trace!("free_position_margin: {margin} on self: {self}");
        assert2::assert!(margin > BaseOrQuote::zero());
        self.debug_assert_state();
        assert2::assert!(self.position_margin >= margin);

        self.position_margin -= margin;
        self.available += margin;
    }

    /// Profit and loss are applied to the available balance.
    #[inline]
    pub fn apply_pnl(&mut self, pnl: BaseOrQuote) {
        trace!("apply_pnl: {pnl}, self: {self}");
        self.available += pnl;
    }
}

#[cfg(test)]
mod test {
    use num::Zero;

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
}
