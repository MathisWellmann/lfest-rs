use std::{
    cmp::Ordering,
    marker::PhantomData,
    num::NonZeroU16,
};

use getset::CopyGetters;
use num::Zero;

use crate::{
    EXPECT_CAPACITY,
    types::{
        Currency,
        LimitOrder,
        MarginCurrency,
        MaxNumberOfActiveOrders,
        Mon,
        Pending,
        Side,
        UserOrderId,
    },
};

pub(crate) struct Bids;

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Cmp<I, D, BaseOrQuote, UserOrderIdT> for Bids
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[inline(always)]
    fn is_same_side(side: Side) -> bool {
        side == Side::Buy
    }

    /// New orders which have a higher price will come later in the vector.
    /// Older orders at the same price level come later in the vector.
    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering {
        use Ordering::*;
        match new_order.limit_price().cmp(&existing_order.limit_price()) {
            Less => Less,
            Equal => {
                match new_order
                    .state()
                    .meta()
                    .ts_exchange_received()
                    .cmp(&existing_order.state().meta().ts_exchange_received())
                {
                    Less => Greater, // Older orders should be later.
                    Equal => Equal,
                    Greater => Less,
                }
            }
            Greater => Greater,
        }
    }
}

pub(crate) struct Asks;

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Cmp<I, D, BaseOrQuote, UserOrderIdT> for Asks
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[inline(always)]
    fn is_same_side(side: Side) -> bool {
        side == Side::Sell
    }

    /// New orders which have a lower price will come later in the vector.
    /// Older orders at the same price level come later in the vector.
    #[inline]
    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering {
        use Ordering::*;
        match new_order.limit_price().cmp(&existing_order.limit_price()) {
            Less => Greater,
            Equal => {
                match new_order
                    .state()
                    .meta()
                    .ts_exchange_received()
                    .cmp(&existing_order.state().meta().ts_exchange_received())
                {
                    Less => Greater,
                    Equal => Equal,
                    Greater => Less,
                }
            }
            Greater => Less,
        }
    }
}

/// Provides the sorting between two limit orders.
/// This differs for bids and asks.
pub(crate) trait Cmp<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    fn is_same_side(side: Side) -> bool;

    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering;
}

// TODO: benchmark this, compare it with `BTreeMap` and const array.
/// Maintains a list of orders, sorted by price and time priority.
/// Optimized for fast insert and removal operations for a small number of orders like 1, 2 or 3.
/// The best ask price and oldest timestamp order will be at the last index.
/// The best bid price and oldest timestamp order will be at the last index.
/// Bids and asks are stored separately, hence the `SideT` generic.
#[derive(Debug, PartialEq, Eq, CopyGetters)]
pub(crate) struct SortedOrders<I, const D: u8, BaseOrQuote, UserOrderIdT, SideT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    orders: Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
    #[getset(get = "pub(crate)")]
    notional_sum: BaseOrQuote::PairedCurrency,
    _side: PhantomData<SideT>,
}

/// A clone impl which retains the capacity as we rely on that assumption downstream.
impl<I, const D: u8, BaseOrQuote, UserOrderIdT, SideT> Clone
    for SortedOrders<I, D, BaseOrQuote, UserOrderIdT, SideT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    fn clone(&self) -> Self {
        let mut orders = self.orders.clone();
        orders.reserve_exact(self.orders.capacity() - self.orders.len());
        assert_eq!(orders.capacity(), self.orders.capacity());
        Self {
            orders,
            notional_sum: self.notional_sum,
            _side: PhantomData,
        }
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT, SideT>
    SortedOrders<I, D, BaseOrQuote, UserOrderIdT, SideT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
    SideT: Cmp<I, D, BaseOrQuote, UserOrderIdT>,
{
    pub(crate) fn with_capacity(cap: NonZeroU16) -> Self {
        Self {
            orders: Vec::with_capacity(cap.get().into()),
            notional_sum: Zero::zero(),
            _side: PhantomData,
        }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.orders.len()
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    #[inline(always)]
    pub(crate) fn best(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders.last()
    }

    #[inline(always)]
    pub(crate) fn best_mut(
        &mut self,
    ) -> Option<&mut LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders.last_mut()
    }

    #[inline(always)]
    pub(crate) fn pop_best(
        &mut self,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders.pop().and_then(|order| {
            self.notional_sum -= order.notional();
            Some(order)
        })
    }

    #[inline]
    pub(crate) fn try_insert(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        debug_assert!(SideT::is_same_side(order.side()));

        if self.orders.len() >= self.orders.capacity() {
            debug_assert!(self.orders.capacity() > 0);
            return Err(MaxNumberOfActiveOrders(
                self.orders
                    .capacity()
                    .try_into()
                    .expect("Will not truncate"),
            ));
        }
        self.notional_sum += order.notional();

        // Find location to insert so that orders remain ordered.
        /*
        use std::cmp::Ordering::*;
        let idx = self
            .orders
            .iter()
            .position(|existing| matches!(SideT::cmp(existing, &order), Less | Equal))
            .unwrap_or(self.orders.len());
        self.orders.insert(idx, order);
        */
        self.orders
            .push_within_capacity(order)
            .expect(EXPECT_CAPACITY);
        self.orders.sort_by(|a, b| SideT::cmp(a, b));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        types::{
            BaseCurrency,
            ExchangeOrderMeta,
            QuoteCurrency,
        },
        utils::NoUserOrderId,
    };

    #[test]
    fn sorted_orders_bids() {
        let mut bids =
            SortedOrders::<i64, 6, BaseCurrency<_, 6>, NoUserOrderId, Bids>::with_capacity(
                NonZeroU16::new(3).unwrap(),
            );
        assert_eq!(bids.notional_sum, Zero::zero());
        assert_eq!(bids.orders.len(), 0);
        assert_eq!(bids.len(), 0);
        assert!(bids.is_empty());
        assert_eq!(bids.best(), None);

        let order_0 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let pending_0 = order_0.into_pending(meta);
        bids.try_insert(pending_0.clone()).unwrap();
        assert_eq!(bids.notional_sum, QuoteCurrency::new(100, 0));
        assert_eq!(bids.orders.len(), 1);
        assert_eq!(bids.len(), 1);
        assert!(!bids.is_empty());
        assert_eq!(bids.best(), Some(&pending_0));

        let order_1 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 1.into());
        let pending_1 = order_1.into_pending(meta);
        bids.try_insert(pending_1.clone()).unwrap();
        dbg!(&bids.orders);
        assert_eq!(bids.notional_sum, QuoteCurrency::new(199, 0));
        assert_eq!(bids.orders.len(), 2);
        assert_eq!(bids.len(), 2);
        assert!(!bids.is_empty());
        assert_eq!(bids.best(), Some(&pending_0));

        let order_2 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(101, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 2.into());
        let pending_2 = order_2.into_pending(meta);
        bids.try_insert(pending_2.clone()).unwrap();
        assert_eq!(bids.notional_sum, QuoteCurrency::new(300, 0));
        assert_eq!(bids.orders.len(), 3);
        assert_eq!(bids.len(), 3);
        assert!(!bids.is_empty());
        assert_eq!(bids.best(), Some(&pending_2));

        assert_eq!(bids.pop_best(), Some(pending_2.clone()));
        assert_eq!(bids.notional_sum, QuoteCurrency::new(199, 0));
        assert_eq!(bids.orders.len(), 2);
        assert_eq!(bids.len(), 2);

        let order_3 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 3.into());
        let pending_3 = order_3.into_pending(meta);
        bids.try_insert(pending_3.clone()).unwrap();
        assert_eq!(bids.notional_sum, QuoteCurrency::new(299, 0));
        assert_eq!(bids.orders.len(), 3);
        assert_eq!(bids.len(), 3);
        assert!(!bids.is_empty());
        assert_eq!(
            bids.best(),
            Some(&pending_0),
            "order_2 has the same price, but an earlier timestamp."
        );

        assert!(bids.try_insert(pending_3).is_err());
    }

    #[test]
    fn sorted_orders_asks() {
        let mut asks =
            SortedOrders::<i64, 6, BaseCurrency<_, 6>, NoUserOrderId, Asks>::with_capacity(
                NonZeroU16::new(3).unwrap(),
            );
        assert_eq!(asks.notional_sum, Zero::zero());
        assert_eq!(asks.orders.len(), 0);
        assert_eq!(asks.len(), 0);
        assert!(asks.is_empty());
        assert_eq!(asks.best(), None);

        let order_0 = LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let pending_0 = order_0.into_pending(meta);
        asks.try_insert(pending_0.clone()).unwrap();
        assert_eq!(asks.notional_sum, QuoteCurrency::new(100, 0));
        assert_eq!(asks.orders.len(), 1);
        assert_eq!(asks.len(), 1);
        assert!(!asks.is_empty());
        assert_eq!(asks.best(), Some(&pending_0));

        let order_1 = LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(101, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 1.into());
        let pending_1 = order_1.into_pending(meta);
        asks.try_insert(pending_1.clone()).unwrap();
        assert_eq!(asks.notional_sum, QuoteCurrency::new(201, 0));
        assert_eq!(asks.orders.len(), 2);
        assert_eq!(asks.len(), 2);
        assert!(!asks.is_empty());
        assert_eq!(asks.best(), Some(&pending_0));

        let order_2 = LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(99, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 2.into());
        let pending_2 = order_2.into_pending(meta);
        asks.try_insert(pending_2.clone()).unwrap();
        dbg!(&asks.orders);
        assert_eq!(asks.notional_sum, QuoteCurrency::new(300, 0));
        assert_eq!(asks.orders.len(), 3);
        assert_eq!(asks.len(), 3);
        assert!(!asks.is_empty());
        assert_eq!(asks.best(), Some(&pending_2));

        assert_eq!(asks.pop_best(), Some(pending_2));
        assert_eq!(asks.notional_sum, QuoteCurrency::new(201, 0));
        assert_eq!(asks.orders.len(), 2);
        assert_eq!(asks.len(), 2);

        let order_3 = LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 3.into());
        let pending_3 = order_3.into_pending(meta);
        asks.try_insert(pending_3.clone()).unwrap();
        assert_eq!(asks.notional_sum, QuoteCurrency::new(301, 0));
        assert_eq!(asks.orders.len(), 3);
        assert_eq!(asks.len(), 3);
        assert!(!asks.is_empty());
        assert_eq!(asks.best(), Some(&pending_0));
    }
}
