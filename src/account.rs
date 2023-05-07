use hashbrown::HashMap;

use crate::{
    market_state::MarketState,
    position::Position,
    types::{Currency, Error, Leverage, MarginCurrency, Order, Result},
};

#[derive(Debug, Clone)]
/// The users account
/// Generic over:
/// S: The `Currency` representing the order quantity
pub struct Account<M>
where
    M: Currency + MarginCurrency,
{
    pub(crate) wallet_balance: M,
    pub(crate) position: Position<M>,
    active_limit_orders: HashMap<u64, Order<M::PairedCurrency>>,
    // Maps the `user_order_id` to the internal order nonce
    lookup_order_nonce_from_user_order_id: HashMap<u64, u64>,
    executed_orders: Vec<Order<M::PairedCurrency>>,
    next_order_id: u64,
}

impl<M> Account<M>
where
    M: Currency + MarginCurrency,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: M, leverage: Leverage) -> Self {
        let position = Position::new(leverage);

        Self {
            wallet_balance: starting_balance,
            position,
            active_limit_orders: HashMap::new(),
            lookup_order_nonce_from_user_order_id: HashMap::new(),
            executed_orders: vec![],
            next_order_id: 0,
        }
    }

    /// Return a reference to the accounts position.
    #[inline(always)]
    pub fn position(&self) -> &Position<M> {
        &self.position
    }

    /// Return the current wallet balance of the account.
    #[inline(always)]
    pub fn wallet_balance(&self) -> M {
        self.wallet_balance
    }

    /// Return the available balance of the `Account`
    #[inline(always)]
    pub fn available_balance(&self) -> M {
        // TODO - order_margin
        warn!("order_margin not included in `available_balance` calculation!");
        self.wallet_balance - self.position.position_margin
    }

    /// Allows the user to update their desired leverage.
    /// This will deposit or release variation margin from the position if any.
    ///
    /// # Returns:
    /// If Err, the account is unable to provide enough variation margin for the desired leverage.
    pub fn update_desired_leverage(&mut self, leverage: Leverage) -> Result<()> {
        todo!()
    }

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
    pub fn cancel_order_by_user_id(
        &mut self,
        user_order_id: u64,
    ) -> Result<Order<M::PairedCurrency>> {
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
    pub(crate) fn append_limit_order(&mut self, order: Order<M::PairedCurrency>) {
        debug!("append_limit_order: order: {:?}", order);

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
    pub fn cancel_order(&mut self, order_id: u64) -> Result<Order<M::PairedCurrency>> {
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
    pub fn active_limit_orders(&self) -> &HashMap<u64, Order<M::PairedCurrency>> {
        &self.active_limit_orders
    }

    /// Return recently executed orders
    /// and clear them afterwards
    pub(crate) fn executed_orders(&mut self) -> Vec<Order<M::PairedCurrency>> {
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
