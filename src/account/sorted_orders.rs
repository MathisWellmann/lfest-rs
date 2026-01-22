//! Contains a data structure for maintaining an ordered list of limit orders,
//! optimized for a small number of active ones.

use std::{
    cmp::Ordering,
    marker::PhantomData,
    num::NonZeroU16,
};

use getset::CopyGetters;
use num::Zero;

use crate::types::{
    Currency,
    Filled,
    LimitOrder,
    MarginCurrency,
    MaxNumberOfActiveOrders,
    Mon,
    OrderId,
    Pending,
    Side,
    TimestampNs,
    UserOrderId,
};

// TODO: move `Bids`,`Asks` and `Cmp` to its own file.
/// zero-sized marker struct indicating sorting for bids.
#[derive(Debug, PartialEq, Eq)]
pub struct Bids;

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
    #[inline(always)]
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

/// zero-sized marker struct indicating sorting for asks.
#[derive(Debug, PartialEq, Eq)]
pub struct Asks;

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
    #[inline(always)]
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
pub trait Cmp<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// If `true`, the side is the same as the marker struct implementing this function.
    /// Used for asserting that only limit orders of the correct side are included.
    fn is_same_side(side: Side) -> bool;

    /// Compare a new order with an existing one for ordering them appropriately.
    /// This implementation differs for `Bids` and `Asks` as the best price and oldest order is always at the last vector position.
    fn cmp(
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        existing_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Ordering;
}

// TODO: compare it with `BTreeMap` and const array implementations.
/// Maintains a list of orders, sorted by price and time priority.
/// Optimized for fast insert and removal operations for a small number of orders like 1, 2 or 3.
/// The best ask price and oldest timestamp order will be at the last index.
/// The best bid price and oldest timestamp order will be at the last index.
/// Bids and asks are stored separately, hence the `SideT` generic.
#[derive(Debug, PartialEq, Eq, CopyGetters)]
pub struct SortedOrders<I, const D: u8, BaseOrQuote, UserOrderIdT, SideT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    orders: Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
    #[getset(get_copy = "pub(crate)")]
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
        debug_assert_eq!(orders.capacity(), self.orders.capacity());
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
    /// Create a new instance with a fixed capacity.
    /// This capacity is retained across `.clone()` calls as well.
    pub fn with_capacity(cap: NonZeroU16) -> Self {
        Self {
            orders: Vec::with_capacity(cap.get().into()),
            notional_sum: Zero::zero(),
            _side: PhantomData,
        }
    }

    #[inline(always)]
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.orders.len()
    }

    #[inline(always)]
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    #[inline(always)]
    #[must_use]
    pub(crate) fn best(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders.last()
    }

    /// Fill the best limit order,
    /// popping it if fully filled
    /// and returning it in the `Filled` state.
    #[inline]
    #[must_use]
    pub(crate) fn fill_best(
        &mut self,
        filled_quantity: BaseOrQuote,
        ts_ns: TimestampNs,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>> {
        self.orders
            .pop_if(|order| {
                let old_notional = order.notional();
                order.fill(filled_quantity);
                let new_notional = order.notional();

                let notional_delta = new_notional - old_notional;
                assert2::debug_assert!(
                    notional_delta < Zero::zero(),
                    "Filling and order reduces the remaining notional value"
                );
                self.notional_sum += notional_delta;
                assert2::debug_assert!(self.notional_sum >= Zero::zero());

                order.remaining_quantity().is_zero()
            })
            .and_then(|order| Some(order.into_filled(ts_ns)))
    }

    /// Get a `LimitOrder` by the given `OrderId` if any.
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_by_id(
        &self,
        order_id: OrderId,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders.iter().find(|order| order.id() == order_id)
    }

    /// Remove a limit order based on its `OrderId`
    #[inline(always)]
    #[must_use]
    pub fn remove_by_id(
        &mut self,
        order_id: OrderId,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders
            .iter()
            .position(|order| order.id() == order_id)
            .and_then(|idx| {
                let order = self.orders.remove(idx);
                self.notional_sum -= order.notional();
                assert2::debug_assert!(self.notional_sum >= Zero::zero());
                Some(order)
            })
    }

    #[inline(always)]
    #[must_use]
    pub(crate) fn remove_by_user_id(
        &mut self,
        uid: UserOrderIdT,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.orders
            .iter()
            .position(|order| order.user_order_id() == uid)
            .and_then(|idx| {
                let order = self.orders.remove(idx);
                self.notional_sum -= order.notional();
                assert2::debug_assert!(self.notional_sum >= Zero::zero());
                Some(order)
            })
    }

    /// Insert a new limit orders if there is enough capacity,
    /// otherwise return an error.
    #[inline]
    pub fn try_insert(
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

        use std::cmp::Ordering::*;
        let idx = self
            .orders
            .iter()
            .position(|existing| matches!(SideT::cmp(&order, existing), Less | Equal))
            .unwrap_or(self.orders.len());
        self.orders.insert(idx, order);
        debug_assert_eq!(
            {
                let mut cloned = self.orders.clone();
                cloned.sort_by(|a, b| SideT::cmp(a, b));
                cloned
            },
            self.orders
        );

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

        assert_eq!(
            bids.fill_best(BaseCurrency::new(1, 0), 3.into())
                .unwrap()
                .id(),
            2.into()
        );
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

        assert_eq!(
            asks.fill_best(BaseCurrency::new(1, 0), 3.into())
                .unwrap()
                .id(),
            2.into()
        );
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

    #[test]
    #[should_panic]
    fn sorted_orders_bids_should_panic() {
        let mut bids =
            SortedOrders::<i64, 6, BaseCurrency<_, 6>, NoUserOrderId, Bids>::with_capacity(
                NonZeroU16::new(3).unwrap(),
            );
        let order_0 = LimitOrder::new(
            Side::Sell,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let pending_0 = order_0.into_pending(meta);
        // Should panic irregardless of the result.
        let _ = bids.try_insert(pending_0.clone());
    }

    #[test]
    #[should_panic]
    fn sorted_orders_asks_should_panic() {
        let mut asks =
            SortedOrders::<i64, 6, BaseCurrency<_, 6>, NoUserOrderId, Asks>::with_capacity(
                NonZeroU16::new(3).unwrap(),
            );
        let order_0 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let pending_0 = order_0.into_pending(meta);
        // Should panic irregardless of the result.
        let _ = asks.try_insert(pending_0.clone());
    }

    #[test]
    fn sorted_orders_clone() {
        let mut bids =
            SortedOrders::<i64, 6, BaseCurrency<_, 6>, NoUserOrderId, Bids>::with_capacity(
                NonZeroU16::new(3).unwrap(),
            );
        let order_0 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let pending_0 = order_0.into_pending(meta);
        bids.try_insert(pending_0.clone()).unwrap();

        let bids_clone = bids.clone();
        assert_eq!(bids.orders, bids_clone.orders);
        assert_eq!(bids.notional_sum, bids_clone.notional_sum);
        assert_eq!(bids.orders.capacity(), bids_clone.orders.capacity());
    }

    #[test]
    fn sorted_orders_remove() {
        let cap = 10;
        let mut bids =
            SortedOrders::<i64, 6, BaseCurrency<_, 6>, NoUserOrderId, Bids>::with_capacity(
                NonZeroU16::new(cap).unwrap(),
            );
        for i in 0..cap {
            let order_0 = LimitOrder::new(
                Side::Buy,
                QuoteCurrency::new(100, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            let pending_0 = order_0.into_pending(meta);
            bids.try_insert(pending_0.clone()).unwrap();
            assert_eq!(
                bids.notional_sum,
                QuoteCurrency::new(100 * (i as i64 + 1), 0)
            );
        }
        assert_eq!(bids.notional_sum, QuoteCurrency::new(100 * cap as i64, 0));

        for i in 0..cap {
            let i = i as u64;
            assert_eq!(bids.remove_by_id(i.into()).unwrap().id(), i.into());
            assert!(bids.remove_by_id(i.into()).is_none());
            assert_eq!(
                bids.notional_sum,
                QuoteCurrency::new((100 * cap as i64) - 100 * (i as i64 + 1), 0)
            );
        }
        assert!(bids.is_empty());
    }
}
