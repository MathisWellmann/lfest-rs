use hashbrown::HashMap;

use crate::{
    market_state::MarketState,
    prelude::Error,
    types::{Currency, Order, Result},
};

/// Super crude `MatchingEngine` that only holds orders for a single user account.
#[derive(Debug, Clone, Default)]
pub struct MatchingEngine<S> {
    active_limit_orders: HashMap<u64, Order<S>>,
    // Maps the `user_order_id` to the internal order nonce
    lookup_order_nonce_from_user_order_id: HashMap<u64, u64>,
    executed_orders: Vec<Order<S>>,
    next_order_id: u64,
}

impl<S> MatchingEngine<S>
where
    S: Currency,
{
    /// The number of currently active limit orders
    #[inline(always)]
    pub(crate) fn num_active_limit_orders(&self) -> usize {
        self.active_limit_orders.len()
    }

    /// Cancel an active order based on the user_order_id of an Order
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    pub fn cancel_order_by_user_id(&mut self, user_order_id: u64) -> Result<Order<S>> {
        debug!("cancel_order_by_user_id: user_order_id: {}", user_order_id);
        let id: u64 = match self
            .lookup_order_nonce_from_user_order_id
            .remove(&user_order_id)
        {
            None => return Err(Error::UserOrderIdNotFound),
            Some(id) => id,
        };
        self.cancel_order(id)
    }

    /// Append a new limit order as active order
    #[deprecated]
    pub(crate) fn append_limit_order(&mut self, order: Order<S>, order_margin: S::PairedCurrency) {
        debug!(
            "append_limit_order: order: {:?}, order_margin: {}",
            order, order_margin
        );

        // self.account_tracker.log_limit_order_submission();
        let order_id = order.id();
        let user_order_id = *order.user_order_id();
        match self.active_limit_orders.insert(order_id, order) {
            None => {}
            Some(_) => warn!(
                "there already was an order with this id in active_limit_orders. \
            This should not happen as order id should be incrementing"
            ),
        };
        match user_order_id {
            None => {}
            Some(user_order_id) => {
                self.lookup_order_nonce_from_user_order_id
                    .insert(user_order_id, order_id);
            }
        };
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Result<Order<S>> {
        debug!("cancel_order: {}", order_id);
        let removed_order = match self.active_limit_orders.remove(&order_id) {
            None => return Err(Error::OrderIdNotFound),
            Some(o) => o,
        };

        // self.account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Return the currently active limit orders
    #[inline(always)]
    pub fn active_limit_orders(&self) -> &HashMap<u64, Order<S>> {
        &self.active_limit_orders
    }

    /// Return recently executed orders
    /// and clear them afterwards
    pub(crate) fn executed_orders(&mut self) -> Vec<Order<S>> {
        let exec_orders = self.executed_orders.clone();
        self.executed_orders.clear();

        exec_orders
    }

    /// Check if any active orders have been triggered by the most recent price
    /// action method is called after new external data has been consumed
    pub(crate) fn handle_resting_orders(&mut self, market_state: &MarketState) {
        // self.active_limit_orders()
        //     .iter()
        //     .map(|(i, _)| *i)
        //     .for_each(|v| self.handle_limit_order());
    }

    fn handle_limit_order(&mut self) {
        todo!()
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> u64 {
        self.next_order_id += 1;
        self.next_order_id - 1
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn matching_engine_assigns_order_ids() {
        todo!()
    }
}
