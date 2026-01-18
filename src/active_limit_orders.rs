use std::num::NonZeroU16;

use const_decimal::Decimal;
use getset::Getters;
use num::{
    One,
    Zero,
};
use tracing::{
    debug,
    trace,
};

use crate::{
    prelude::Position,
    types::{
        Balances,
        CancelBy,
        Currency,
        LimitOrder,
        MarginCurrency,
        MaxNumberOfActiveOrders,
        Mon,
        OrderId,
        OrderIdNotFound,
        Pending,
        Side::{
            self,
            *,
        },
        UserOrderId,
        price_time_priority_ordering,
    },
    utils::order_margin,
};

// TODO: rename to `OrderBook`
/// The datatype that holds the active limit orders of a user.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currency.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderIdT`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Debug, Default, PartialEq, Eq, Getters)]
pub struct ActiveLimitOrders<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Stores all the active buy orders in ascending price, time priority.
    /// Best bid is the last element.
    #[getset(get = "pub")]
    bids: Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
    bids_notional: BaseOrQuote::PairedCurrency,

    /// Stores all the active sell orders in ascending price, time priority.
    /// Best ask is the first element.
    #[getset(get = "pub")]
    asks: Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
    asks_notional: BaseOrQuote::PairedCurrency,
}

/// A clone impl which retains the capacity as we rely on that assumption downstream.
impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Clone
    for ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    fn clone(&self) -> Self {
        let mut bids = self.bids.clone();
        bids.reserve_exact(self.bids.capacity() - self.bids.len());
        assert_eq!(bids.capacity(), self.bids.capacity());
        let mut asks = self.asks.clone();
        asks.reserve_exact(self.asks.capacity() - self.asks.len());
        assert_eq!(asks.capacity(), self.asks.capacity());
        Self {
            bids,
            asks,
            bids_notional: self.bids_notional,
            asks_notional: self.asks_notional,
        }
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> std::fmt::Display
    for ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ActiveLimitOrders:")?;
        for order in self.bids.iter() {
            writeln!(f, "{order}")?;
        }
        for order in self.asks.iter() {
            writeln!(f, "{order}")?;
        }
        Ok(())
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new order book instance with a maximum capacity for bids and asks.
    /// The `max_active_orders` must be non zero as we need at least space for one limit order.
    pub(crate) fn with_capacity(max_active_orders: NonZeroU16) -> Self {
        let cap: usize = max_active_orders.get().into();
        Self {
            bids: Vec::with_capacity(cap),
            bids_notional: Zero::zero(),
            asks: Vec::with_capacity(cap),
            asks_notional: Zero::zero(),
        }
    }

    /// Get the number of active limit orders.
    #[inline]
    pub fn num_active(&self) -> usize {
        self.bids.len() + self.asks.len()
    }

    /// `true` is there are no active orders.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// Peek at the best bid limit order.
    #[inline]
    pub(crate) fn peek_best_bid(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        // The last element in `bids` has the highest price and oldest timestamp.
        let opt_out = self.bids().last();

        // Make sure bids are sorted by time and price priority.
        debug_assert!(
            if let Some(order) = opt_out {
                self.bids
                    .iter()
                    .all(|bid| order.limit_price() >= bid.limit_price())
            } else {
                true
            },
            "The order {opt_out:?} must be the best bid in bids {:?}",
            self.bids
        );

        opt_out
    }

    /// Peek at the best ask limit order.
    #[inline]
    pub(crate) fn peek_best_ask(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        // The first element in `asks` has the lowest price and oldest timestamp.
        let opt_out = self.asks().first();

        // Make sure asks are sorted by time and price priority.
        debug_assert!(
            if let Some(order) = opt_out {
                self.asks
                    .iter()
                    .all(|ask| order.limit_price() <= ask.limit_price())
            } else {
                true
            },
            "The order {opt_out:?} must be the best ask in asks {:?}",
            self.asks
        );

        opt_out
    }

    /// Try to insert a new `LimitOrder` into the order book.
    /// Returns an error if the maximum capacity is reached.
    pub(crate) fn try_insert(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        position: &Position<I, D, BaseOrQuote>,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        init_margin_req: Decimal<I, D>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        use std::cmp::Ordering::*;

        let side = order.side();
        let notional = order.notional();
        match side {
            Buy => {
                if self.bids.len() >= self.bids.capacity() {
                    debug_assert!(self.bids.capacity() > 0);
                    return Err(MaxNumberOfActiveOrders(
                        self.bids.capacity().try_into().expect("Will not truncate"),
                    ));
                }
                // Find location to insert so that bids remain ordered.
                let idx = self
                    .bids
                    .iter()
                    .position(|bid| {
                        matches!(price_time_priority_ordering(&order, bid), Less | Equal)
                    })
                    .unwrap_or(self.bids.len());
                trace!("insert bid {order} at idx {idx}, bids: {:?}", self.bids);
                self.bids.insert(idx, order)
            }
            Sell => {
                if self.asks.len() >= self.asks.capacity() {
                    debug_assert!(self.asks.capacity() > 0);
                    return Err(MaxNumberOfActiveOrders(
                        self.bids.capacity().try_into().expect("Will not truncate"),
                    ));
                }
                let idx = self
                    .asks
                    .iter()
                    .position(|bid| {
                        matches!(price_time_priority_ordering(&order, bid), Less | Equal)
                    })
                    .unwrap_or(self.asks.len());
                trace!("insert ask {order} at idx {idx}, asks: {:?}", self.asks);
                self.asks.insert(idx, order)
            }
        }

        let current_om = self.order_margin(init_margin_req, position);
        match side {
            Buy => self.bids_notional += notional,
            Sell => self.asks_notional += notional,
        }

        // Update balances
        let new_order_margin = self.order_margin(init_margin_req, position);
        debug!("new_order_margin: {new_order_margin}");
        if new_order_margin > current_om {
            let margin = new_order_margin - current_om;
            assert2::debug_assert!(margin >= BaseOrQuote::PairedCurrency::zero());
            let success = balances.try_reserve_order_margin(margin);
            assert!(success, "Can place order");
        }
        Ok(())
    }

    pub(crate) fn order_margin(
        &self,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        order_margin(
            self.bids_notional,
            self.asks_notional,
            init_margin_req,
            position,
        )
    }

    /// Get the order margin if a new order were to be added.
    pub(crate) fn order_margin_with_order(
        &self,
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(init_margin_req > Decimal::zero());
        assert2::debug_assert!(init_margin_req <= Decimal::one());

        let mut buy_notional = self.bids_notional;
        let mut sell_notional = self.asks_notional;
        let new_notional = new_order.notional();
        match new_order.side() {
            Buy => buy_notional += new_notional,
            Sell => sell_notional += new_notional,
        }

        order_margin(buy_notional, sell_notional, init_margin_req, position)
    }

    /// Update an existing `LimitOrder`.
    /// Returns the old order
    #[must_use]
    pub(crate) fn update(
        &mut self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>> {
        let active_order = match order.side() {
            Buy => self.bids.iter_mut().find(|o| o.id() == order.id()),
            Sell => self.asks.iter_mut().find(|o| o.id() == order.id()),
        }
        .expect("Order must have been active before updating it");
        debug_assert_ne!(
            active_order, order,
            "An update to an order should not be the same as the existing one"
        );
        assert2::debug_assert!(
            order.remaining_quantity() < active_order.remaining_quantity(),
            "An update to an existing order must mean the new order has less quantity than the tracked order."
        );
        debug_assert_eq!(order.id(), active_order.id());
        Self::assert_limit_order_update_reduces_qty(active_order, order);

        let old_order = active_order.clone();
        *active_order = order.clone();

        old_order
    }

    pub(crate) fn assert_limit_order_update_reduces_qty(
        active_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        updated_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) {
        // when an existing limit order is updated for margin purposes here, its quantity is always reduced.
        assert2::debug_assert!(
            active_order.remaining_quantity() - updated_order.remaining_quantity()
                > BaseOrQuote::zero()
        );
    }

    /// Get a `LimitOrder` by the given `OrderId` if any.
    /// Optimized to be fast for small number of active limit orders.
    #[inline]
    pub fn get_by_id(
        &self,
        order_id: OrderId,
        side: Side,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        match side {
            Buy => self.bids.iter().find(|order| order.id() == order_id),
            Sell => self.asks.iter().find(|order| order.id() == order_id),
        }
    }

    /// Remove an active `LimitOrder` based on its order id.
    #[inline]
    fn remove_by_id(
        &mut self,
        id: OrderId,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        if let Some(pos) = self.bids.iter_mut().position(|order| order.id() == id) {
            return Some(self.bids.remove(pos));
        }
        if let Some(pos) = self.asks.iter_mut().position(|order| order.id() == id) {
            return Some(self.asks.remove(pos));
        };
        None
    }

    /// Remove an active `LimitOrder` based on its order id.
    #[inline]
    fn remove_by_user_id(
        &mut self,
        uid: UserOrderIdT,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        if let Some(pos) = self
            .bids
            .iter_mut()
            .position(|order| order.user_order_id() == uid)
        {
            return Some(self.bids.remove(pos));
        }
        if let Some(pos) = self
            .asks
            .iter_mut()
            .position(|order| order.user_order_id() == uid)
        {
            return Some(self.asks.remove(pos));
        };
        None
    }

    /// Remove an order from being tracked for margin purposes.
    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    pub(crate) fn remove_limit_order(
        &mut self,
        by: CancelBy<UserOrderIdT>,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        OrderIdNotFound<UserOrderIdT>,
    > {
        let original_om = self.order_margin(init_margin_req, position);

        debug!("remove_limit_order {by:?}");
        use CancelBy::*;
        let removed_order = match by {
            OrderId(order_id) => self
                .remove_by_id(order_id)
                .ok_or(OrderIdNotFound::OrderId(order_id))?,
            UserOrderId(user_order_id) => self
                .remove_by_user_id(user_order_id)
                .ok_or(OrderIdNotFound::UserOrderId(user_order_id))?,
        };

        match removed_order.side() {
            Buy => {
                self.bids_notional -= removed_order.notional();
                assert2::debug_assert!(self.bids_notional >= Zero::zero());
            }
            Sell => {
                self.asks_notional -= removed_order.notional();
                assert2::debug_assert!(self.asks_notional >= Zero::zero());
            }
        }

        // Update balances
        let new_order_margin = self.order_margin(init_margin_req, position);
        assert2::debug_assert!(
            new_order_margin <= original_om,
            "When removing a limit order, the new order margin is smaller or equal the old order margin"
        );
        if new_order_margin < original_om {
            let margin = original_om - new_order_margin;
            assert2::debug_assert!(margin >= Zero::zero());
            balances.free_order_margin(margin);
        }

        Ok(removed_order)
    }

    /// fill an existing limit order, reduces order margin.
    /// # Panics:
    /// panics if the order id was not found.
    pub(crate) fn fill_order(
        &mut self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        position: &Position<I, D, BaseOrQuote>,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        init_margin_req: Decimal<I, D>,
    ) {
        let original_om = self.order_margin(init_margin_req, position);

        trace!("OrderMargin.update {order:?}");
        let notional = order.notional();
        assert2::debug_assert!(notional > Zero::zero());
        let old_order = self.update(order);
        let notional_delta = notional - old_order.notional();

        match old_order.side() {
            Buy => {
                self.bids_notional += notional_delta;
                assert2::debug_assert!(self.bids_notional >= Zero::zero());
            }
            Sell => {
                self.asks_notional += notional_delta;
                assert2::debug_assert!(self.asks_notional >= Zero::zero());
            }
        }

        // Update balances
        let new_order_margin = self.order_margin(init_margin_req, position);
        assert2::debug_assert!(
            new_order_margin <= original_om,
            "The order margin does not increase with a filled limit order event."
        );
        if new_order_margin < original_om {
            let margin_delta = original_om - new_order_margin;
            assert2::debug_assert!(margin_delta > Zero::zero());
            balances.free_order_margin(margin_delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use const_decimal::Decimal;
    use num::{
        One,
        Zero,
    };
    use rand::Rng;

    use super::ActiveLimitOrders;
    use crate::{
        DECIMALS,
        prelude::{
            Position::{
                self,
                *,
            },
            PositionInner,
        },
        test_fee_maker,
        types::{
            Balances,
            BaseCurrency,
            CancelBy,
            Currency,
            ExchangeOrderMeta,
            Leverage,
            LimitOrder,
            LimitOrderFill,
            QuoteCurrency,
            Side::{
                self,
                *,
            },
            TimestampNs,
            price_time_priority_ordering,
        },
        utils::NoUserOrderId,
    };

    #[test]
    fn active_limit_orders_remove_by_id() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();
        let mut book =
            ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>::with_capacity(
                NonZeroU16::new(10).unwrap(),
            );

        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.get_by_id(0.into(), Buy), Some(&order));
        assert_eq!(book.remove_by_id(0.into()), Some(order));
        assert_eq!(book.get_by_id(0.into(), Buy), None);
        assert_eq!(book.remove_by_id(0.into()), None);

        let order = LimitOrder::new(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.get_by_id(1.into(), Sell), Some(&order));
        assert_eq!(book.remove_by_id(1.into()), Some(order));
        assert_eq!(book.get_by_id(1.into(), Sell), None);
        assert_eq!(book.remove_by_id(1.into()), None);
    }

    #[test]
    fn active_limit_orders_remove_by_user_order_id() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();
        let mut book = ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, i32>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let order = LimitOrder::new_with_user_order_id(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
            100,
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.remove_by_user_id(100), Some(order));
        assert_eq!(book.remove_by_user_id(100), None);

        let order = LimitOrder::new_with_user_order_id(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
            200,
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.remove_by_user_id(200), Some(order));
        assert_eq!(book.remove_by_user_id(200), None);
    }

    #[test]
    fn active_limit_orders_clone() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();

        let cap = 10;
        let mut book =
            ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>::with_capacity(
                NonZeroU16::new(10).unwrap(),
            );
        assert_eq!(book.bids().len(), 0);
        assert_eq!(book.asks().len(), 0);
        assert_eq!(book.bids().capacity(), cap);
        assert_eq!(book.asks().capacity(), cap);

        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        book = book.clone();
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.asks().len(), 0);
        assert_eq!(book.bids().capacity(), cap);
        assert_eq!(book.asks().capacity(), cap);

        let order = LimitOrder::new(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        book = book.clone();
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.asks().len(), 1);
        assert_eq!(book.bids().capacity(), cap);
        assert_eq!(book.asks().capacity(), cap);
    }

    #[test]
    fn active_limit_orders_get_by_id() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();

        let mut book =
            ActiveLimitOrders::<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId>::with_capacity(
                NonZeroU16::new(10).unwrap(),
            );
        for i in 0..10 {
            assert_eq!(book.get_by_id(i.into(), Buy), None);
        }
        for i in 0..10 {
            assert_eq!(book.get_by_id(i.into(), Sell), None);
        }

        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.get_by_id(0.into(), Buy).unwrap(), &order);
        for i in 1..10 {
            assert_eq!(book.get_by_id(i.into(), Buy), None);
        }

        let order = LimitOrder::new(
            Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.get_by_id(0.into(), Sell).unwrap(), &order);
        for i in 1..10 {
            assert!(book.get_by_id(i.into(), Buy).is_none());
        }
    }

    #[test]
    #[tracing_test::traced_test]
    fn active_limit_orders_insert() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();

        let mut book = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );
        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();

        assert_eq!(book.num_active(), 1);
        let removed = book.remove_by_id(0.into()).unwrap();
        assert_eq!(removed, order);
        assert!(book.is_empty());

        let order_1 = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(200, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 1.into());
        let order_1 = order_1.into_pending(meta);
        book.try_insert(order_1.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.num_active(), 1);
        let removed = book.remove_by_id(1.into()).unwrap();
        assert_eq!(removed, order_1);
        assert!(book.is_empty());

        let mut rng = rand::rng();
        for i in 2..7 {
            let order = LimitOrder::new(
                Buy,
                QuoteCurrency::<i64, 5>::new(rng.random_range(100..500), 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap();
            let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
            let order = order.into_pending(meta);
            book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
                .unwrap();
            let mut sorted = book.bids.clone();
            sorted.sort_by(price_time_priority_ordering);
            assert_eq!(sorted, book.bids);
        }
        assert_eq!(book.num_active(), 5);

        for i in 0..5 {
            let order = LimitOrder::new(
                Sell,
                QuoteCurrency::<i64, 5>::new(rng.random_range(100..500), 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap();
            let meta = ExchangeOrderMeta::new(i.into(), (i as i64).into());
            let order = order.into_pending(meta);
            book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
                .unwrap();
            let mut sorted = book.asks.clone();
            sorted.sort_by(price_time_priority_ordering);
            assert_eq!(sorted, book.asks);
        }
        assert_eq!(book.num_active(), 10);
    }

    #[test]
    #[tracing_test::traced_test]
    fn active_limit_orders_display() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        let init_margin_req = Decimal::one();

        let mut book = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(3).unwrap(),
        );
        let order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();

        assert_eq!(
            &book.to_string(),
            "ActiveLimitOrders:\nuser_id: NoUserOrderId, limit Buy 5.00000 Base @ 100.00000 Quote, state: Pending { meta: ExchangeOrderMeta { id: OrderId(0), ts_exchange_received: TimestampNs(0) }, filled_quantity: Unfilled }\n"
        );
    }
    #[test]
    fn order_margin_assert_limit_order_reduces_qty() {
        let new_active_order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let active_order = new_active_order.into_pending(meta);

        let mut updated_order = active_order.clone();
        let fee = QuoteCurrency::new(0, 0);
        updated_order.fill(BaseCurrency::new(1, 0), fee, 1.into());

        ActiveLimitOrders::assert_limit_order_update_reduces_qty(&active_order, &updated_order);
    }

    #[test]
    #[should_panic]
    fn order_margin_assert_limit_order_reduces_qty_panic() {
        let new_active_order = LimitOrder::new(
            Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let order_0 = new_active_order.into_pending(meta);

        let mut order_1 = order_0.clone();
        let fee = QuoteCurrency::new(0, 0);
        order_1.fill(BaseCurrency::new(1, 0), fee, 1.into());

        ActiveLimitOrders::assert_limit_order_update_reduces_qty(&order_1, &order_0);
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_no_orders(leverage: u8) {
        let order_margin = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let position = Position::<_, 4, BaseCurrency<i32, 4>>::Neutral;
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_long_no_orders(leverage: u8, position_qty: i32, entry_price: i32) {
        let order_margin = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(1500, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Long(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_short_no_orders(leverage: u8, position_qty: i32, entry_price: i32) {
        let order_margin = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Short(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy, Sell],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_same_side(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        n: usize,
    ) {
        let max_active_orders = NonZeroU16::new(10).unwrap();
        let mut book =
            ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(max_active_orders);

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        orders.iter().for_each(|order| {
            book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
                .unwrap()
        });

        let mult = QuoteCurrency::new(n as i32, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        orders.iter().for_each(|order| {
            let _ = book.remove_limit_order(
                CancelBy::OrderId(order.id()),
                init_margin_req,
                &position,
                &mut balances,
            );
        });
        let om = QuoteCurrency::new(0, 0);
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_opposite_side(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        n: usize,
    ) {
        let mut book = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        buy_orders.iter().for_each(|order| {
            book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
                .unwrap();
        });
        let notional: QuoteCurrency<i32, 4> = buy_orders.iter().map(|o| o.notional()).sum();
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            notional * init_margin_req
        );

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
                .unwrap();
        });

        let mult = QuoteCurrency::new(n as i32, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral,
            ),
            om
        );

        buy_orders.iter().for_each(|order| {
            let _ = book.remove_limit_order(
                CancelBy::OrderId(order.id()),
                init_margin_req,
                &position,
                &mut balances,
            );
        });
        sell_orders.iter().for_each(|order| {
            let _ = book.remove_limit_order(
                CancelBy::OrderId(order.id()),
                init_margin_req,
                &position,
                &mut balances,
            );
        });
        assert_eq!(
            book.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_long_orders_of_same_qty(leverage: u8) {
        let mut book = ActiveLimitOrders::<_, 4, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(3, 0);
        let limit_price = QuoteCurrency::new(100, 0);
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();

        let pos_entry_price = QuoteCurrency::new(90, 0);
        let position = Short(PositionInner::new(qty, pos_entry_price));

        // The limit orders may require more margin.
        let om = QuoteCurrency::convert_from(qty, QuoteCurrency::new(10, 0)) * init_margin_req;

        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy, Sell],
        [70, 90, 110],
        [1, 2, 3]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_update_partial_fills(
        leverage: u8,
        side: Side,
        limit_price: i64,
        qty: i64,
    ) {
        let mut book = ActiveLimitOrders::<_, DECIMALS, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        let notional = order.notional();
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

        book.try_insert(order.clone(), &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.num_active(), 1);
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            notional * init_margin_req
        );

        // Now partially fill the order
        let filled_qty = qty / BaseCurrency::new(2, 0);
        let fee = QuoteCurrency::convert_from(filled_qty, limit_price) * *test_fee_maker().as_ref();
        match order.fill(filled_qty, fee, 0.into()) {
            LimitOrderFill::PartiallyFilled {
                filled_quantity,
                fee: f,
                order_after_fill: _,
            } => {
                assert_eq!(filled_quantity, filled_qty);
                assert_eq!(f, fee);
            }
            LimitOrderFill::FullyFilled { .. } => panic!("Expected `PartiallyFilled`"),
        }
        let remaining_qty = order.remaining_quantity();
        book.fill_order(&order, &position, &mut balances, init_margin_req);
        assert_eq!(book.num_active(), 1);
        assert_eq!(remaining_qty, filled_qty);
        let om = QuoteCurrency::convert_from(remaining_qty, limit_price) * init_margin_req;
        assert_eq!(book.order_margin(init_margin_req, &Neutral), om,);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Neutral;
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();
        let mut book = ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap());

        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let qty = BaseCurrency::<i32, 4>::new(1, 0);
        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);

        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.asks().len(), 0);
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.asks().len(), 1);
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.asks().len(), 2);
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut book = ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap());

        let qty = BaseCurrency::<i64, 5>::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Long(PositionInner::new(qty, entry_price));
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();

        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om,);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(120, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(185, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut book = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::with_capacity(
            NonZeroU16::new(10).unwrap(),
        );

        let qty = BaseCurrency::<i64, DECIMALS>::one();
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Short(PositionInner::new(qty, entry_price));
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let init_margin_req = Decimal::one();

        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::one();
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        assert_eq!(
            book.order_margin(init_margin_req, &position),
            QuoteCurrency::zero()
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        book.try_insert(order, &position, &mut balances, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(book.order_margin(init_margin_req, &position), om);
    }
}
