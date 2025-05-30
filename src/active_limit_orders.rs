use getset::Getters;
use tracing::trace;

use crate::types::{
    Currency, Error, LimitOrder, MarginCurrency, Mon, OrderId, Pending, Side, UserOrderId,
    price_time_priority_ordering,
};

/// The datatype that holds the active limit orders of a user.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currency.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderIdT`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Debug, Clone, PartialEq, Eq, Getters)]
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

    /// Stores all the active sell orders in ascending price, time priority.
    /// Best ask is the first element.
    #[getset(get = "pub")]
    asks: Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
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
    #[inline]
    pub(crate) fn new(max_active_orders: usize) -> Self {
        Self {
            bids: Vec::with_capacity(max_active_orders),
            asks: Vec::with_capacity(max_active_orders),
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

    /// Insert a new `LimitOrder`.
    /// If we did not have this key present, `Ok(None)` is returned.
    /// If we did have this key present, the value is updated, and the old value is returned.
    #[allow(clippy::type_complexity)]
    pub(crate) fn insert(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> crate::Result<
        Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
    > {
        match order.side() {
            Side::Buy => {
                if self.bids.len() >= self.bids.capacity() {
                    return Err(Error::MaxNumberOfActiveOrders);
                }
                // Find the index where to insert it with price, time priority.
                if let Some(existing_order) = self.bids.iter_mut().find(|o| o.id() == order.id()) {
                    let out = existing_order.clone();
                    *existing_order = order;
                    return Ok(Some(out));
                }
                self.bids.push(order);

                self.bids.sort_by(price_time_priority_ordering);
            }
            Side::Sell => {
                if self.asks.len() >= self.asks.capacity() {
                    return Err(Error::MaxNumberOfActiveOrders);
                }
                // Find the index where to insert it with price, time priority.
                if let Some(existing_order) = self.asks.iter_mut().find(|o| o.id() == order.id()) {
                    let out = existing_order.clone();
                    *existing_order = order;
                    return Ok(Some(out));
                }
                self.asks.push(order);

                self.asks.sort_by(price_time_priority_ordering);
            }
        }

        Ok(None)
    }

    /*
    /// The best bid (highest price of all buy orders) if any.
    #[inline(always)]
    pub(crate) fn best_bid(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.bids.last()
    }

    /// The best ask (lowest price of all sell orders) if any.
    #[inline(always)]
    pub(crate) fn best_ask(
        &self,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.asks.get(0)
    }
    */

    /// Get a `LimitOrder` by the given `OrderId` if any.
    /// Optimized to be fast for small number of active limit orders.
    #[inline]
    pub fn get_by_id(
        &self,
        order_id: OrderId,
        side: Side,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        match side {
            Side::Buy => self.bids.iter().find(|order| order.id() == order_id),
            Side::Sell => self.asks.iter().find(|order| order.id() == order_id),
        }
    }

    /// Remove an active `LimitOrder` based on its order id.
    #[inline]
    pub(crate) fn remove_by_id(
        &mut self,
        id: OrderId,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        if let Some(pos) = self.bids.iter_mut().position(|order| order.id() == id) {
            let removed = self.bids.swap_remove(pos);
            trace!("removed bid {removed}");
            return Some(removed);
        } else {
            let pos = self.asks.iter_mut().position(|order| order.id() == id)?;
            let removed = self.asks.swap_remove(pos);
            trace!("removed ask {removed}");
            Some(removed)
        }
    }

    /// Remove an active `LimitOrder` based on its order id.
    #[inline]
    pub(crate) fn remove_by_user_order_id(
        &mut self,
        user_order_id: UserOrderIdT,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        if let Some(pos) = self
            .bids
            .iter_mut()
            .position(|order| order.user_order_id() == user_order_id)
        {
            let removed = self.bids.swap_remove(pos);
            trace!("removed bid {removed}");
            return Some(removed);
        } else {
            let pos = self
                .asks
                .iter_mut()
                .position(|order| order.user_order_id() == user_order_id)?;
            let removed = self.asks.swap_remove(pos);
            trace!("removed ask {removed}");
            Some(removed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ActiveLimitOrders;
    use crate::{
        types::{BaseCurrency, ExchangeOrderMeta, LimitOrder, QuoteCurrency, Side},
        utils::NoUserOrderId,
    };

    #[test]
    fn size_of_optional_reference() {
        // 64 bit system
        assert_eq!(std::mem::size_of::<&i32>(), 8);
        assert_eq!(std::mem::size_of::<Option<&i32>>(), 8);
    }

    #[test]
    fn active_limit_orders_insert() {
        let mut alo = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::new(3);
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        alo.insert(order.clone()).unwrap();
        assert_eq!(alo.num_active(), 1);
    }

    #[test]
    fn active_limit_orders() {
        let mut alo = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::new(3);
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        alo.insert(order.clone()).unwrap();

        assert_eq!(alo.num_active(), 1);
        let removed = alo.remove_by_id(0.into()).unwrap();
        assert_eq!(removed, order);
        assert!(alo.is_empty());

        let order_1 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(200, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 1.into());
        let order_1 = order_1.into_pending(meta);
        alo.insert(order_1.clone()).unwrap();
        assert_eq!(alo.num_active(), 1);
        let removed = alo.remove_by_id(1.into()).unwrap();
        assert_eq!(removed, order_1);
        assert!(alo.is_empty());

        for i in 2..5 {
            let order = LimitOrder::new(
                Side::Buy,
                QuoteCurrency::<i64, 5>::new(200, 0),
                BaseCurrency::new(1, 0),
            )
            .unwrap();
            let meta = ExchangeOrderMeta::new(i.into(), 3.into());
            let order = order.into_pending(meta);
            alo.insert(order.clone()).unwrap();
        }
        assert_eq!(alo.num_active(), 3);
        assert!(alo.insert(order_1).is_err());
    }

    #[test]
    fn active_limit_orders_display() {
        let mut alo = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::new(3);
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        alo.insert(order.clone()).unwrap();

        assert_eq!(
            &alo.to_string(),
            "ActiveLimitOrders:\nuser_id: NoUserOrderId, limit Buy 5.00000 Base @ 100.00000 Quote, state: Pending { meta: ExchangeOrderMeta { id: OrderId(0), ts_ns_exchange_received: TimestampNs(0) }, filled_quantity: Unfilled }\n"
        );
    }
}
