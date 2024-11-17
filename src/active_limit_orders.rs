use crate::types::{
    Currency, Error, LimitOrder, MarginCurrency, Mon, OrderId, Pending, UserOrderIdT,
};

/// The datatype that holds the active limit orders of a user.
/// faster than `hashbrown::HashMap` and optimized for small number of active orders.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currency.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveLimitOrders<I, const D: u8, BaseOrQuote, UserOrderId>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderId: UserOrderIdT,
{
    // Stores all the active orders.
    arena: Vec<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> std::fmt::Display
    for ActiveLimitOrders<I, D, BaseOrQuote, UserOrderId>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderId: UserOrderIdT,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActiveLimitOrders:\n")?;
        for order in self.arena.iter() {
            write!(f, "{order}\n")?;
        }
        Ok(())
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> ActiveLimitOrders<I, D, BaseOrQuote, UserOrderId>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderId: UserOrderIdT,
{
    #[inline]
    pub(crate) fn new(max_active_orders: usize) -> Self {
        Self {
            arena: Vec::with_capacity(max_active_orders),
        }
    }

    /// Get the number of active limit orders.
    #[inline]
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// `true` is there are no active orders.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Insert a new `LimitOrder`.
    /// Optimized for small number of active orders.
    /// If we did not have this key present, `Ok(None)` is returned.
    /// If we did have this key present, the value is updated, and the old value is returned.
    #[inline]
    pub(crate) fn insert(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
    ) -> crate::Result<Option<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>>>
    {
        // check if it exists
        if let Some(existing_order) = self.get_mut_by_id(order.id()) {
            let out = existing_order.clone();
            // update the value and return the one that existed.
            *existing_order = order;
            return Ok(Some(out));
        }

        if self.arena.len() >= self.arena.capacity() {
            return Err(Error::MaxNumberOfActiveOrders);
        }
        self.arena.push(order);

        Ok(None)
    }

    /// Get a `LimitOrder` by the given `OrderId` if any.
    /// Optimized to be fast for small number of active limit orders.
    #[inline]
    pub fn get_by_id(
        &self,
        order_id: OrderId,
    ) -> Option<&LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        self.arena.iter().find(|order| order.id() == order_id)
    }

    /// Get a `LimitOrder` by the given `OrderId` if any.
    /// Optimized to be fast for small number of active limit orders.
    #[inline]
    pub(crate) fn get_mut_by_id(
        &mut self,
        order_id: OrderId,
    ) -> Option<&mut LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        self.arena.iter_mut().find(|order| order.id() == order_id)
    }

    /// Remove an active `LimitOrder` based on its `OrderId`.
    /// Optimized for small number of active orders.
    #[inline]
    pub(crate) fn remove_by_order_id(
        &mut self,
        order_id: OrderId,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        let Some(pos) = self
            .arena
            .iter_mut()
            .position(|order| order.id() == order_id)
        else {
            return None;
        };
        Some(self.arena.swap_remove(pos))
    }

    /// Remove an active `LimitOrder` based on its `UserOrderId`.
    /// Optimized for small number of active orders.
    #[inline]
    pub(crate) fn remove_by_user_order_id(
        &mut self,
        user_order_id: UserOrderId,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        let Some(pos) = self
            .arena
            .iter_mut()
            .position(|order| order.user_order_id() == user_order_id)
        else {
            return None;
        };
        Some(self.arena.swap_remove(pos))
    }

    /// Get an iterator over the active limit orders.
    #[inline]
    pub fn values(
        &self,
    ) -> impl Iterator<Item = &LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>>
    {
        self.arena.iter()
    }

    #[inline]
    pub(crate) fn values_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>>
    {
        self.arena.iter_mut()
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
    fn active_limit_orders() {
        let mut alo = ActiveLimitOrders::<i64, 5, _, NoUserOrderId>::new(3);
        let order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        alo.insert(order.clone()).unwrap();

        assert_eq!(alo.len(), 1);
        assert_eq!(alo.arena[0], order);
        assert_eq!(alo.get_by_id(0.into()), Some(&order));
        assert_eq!(alo.get_mut_by_id(0.into()), Some(&mut order));
        let removed = alo.remove_by_order_id(0.into()).unwrap();
        assert_eq!(removed, order);
        assert!(alo.is_empty());

        let order_1 = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(200, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 1.into());
        let mut order_1 = order_1.into_pending(meta);
        alo.insert(order_1.clone()).unwrap();
        assert_eq!(alo.len(), 1);
        assert_eq!(alo.arena[0], order_1);
        assert!(alo.get_by_id(0.into()).is_none());
        assert!(alo.get_mut_by_id(0.into()).is_none());
        assert_eq!(alo.get_by_id(1.into()), Some(&order_1));
        assert_eq!(alo.get_mut_by_id(1.into()), Some(&mut order_1));
        let removed = alo.remove_by_order_id(1.into()).unwrap();
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
        assert_eq!(alo.len(), 3);
        assert!(alo.insert(order_1).is_err());
    }
}
