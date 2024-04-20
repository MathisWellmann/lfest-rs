use getset::{CopyGetters, Getters};
use hashbrown::HashMap;

use crate::{
    order_margin::compute_order_margin,
    position::Position,
    prelude::AccountTracker,
    types::{Currency, Error, Fee, Leverage, MarginCurrency, Order, OrderType, Result},
};

/// The users account
/// Generic over:
/// S: The `Currency` representing the order quantity
#[derive(Debug, Clone, Getters, CopyGetters)]
pub struct Account<M>
where
    M: Currency + MarginCurrency,
{
    /// The wallet balance of the user denoted in the margin currency.
    /// TODO: I don't really want this to be mutably accessible publically, but for now thats how we roll.
    #[getset(get_copy = "pub", get_mut = "pub")]
    pub(crate) wallet_balance: M,

    /// The current account `Position`.
    #[getset(get = "pub")]
    pub(crate) position: Position<M>,

    /// Maps the order `id` to the actual `Order`.
    #[getset(get = "pub")]
    pub(crate) active_limit_orders: HashMap<u64, Order<M::PairedCurrency>>,

    /// Maps the `user_order_id` to the internal order nonce
    pub(crate) lookup_order_nonce_from_user_order_id: HashMap<u64, u64>,

    maker_fee: Fee,

    /// The current order margin used by the user `Account`.
    #[getset(get_copy = "pub")]
    order_margin: M,
}

#[cfg(test)]
impl<M> Default for Account<M>
where
    M: Currency + MarginCurrency,
{
    fn default() -> Self {
        use crate::prelude::{fee, Dec, Decimal};
        Self {
            wallet_balance: M::new(Dec!(1)),
            position: Position::default(),
            active_limit_orders: HashMap::default(),
            lookup_order_nonce_from_user_order_id: HashMap::default(),
            maker_fee: fee!(0.0),
            order_margin: M::new(Dec!(0)),
        }
    }
}

impl<M> Account<M>
where
    M: Currency + MarginCurrency,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: M, leverage: Leverage, maker_fee: Fee) -> Self {
        let position = Position::new(leverage);

        Self {
            wallet_balance: starting_balance,
            position,
            active_limit_orders: HashMap::new(),
            lookup_order_nonce_from_user_order_id: HashMap::new(),
            maker_fee,
            order_margin: M::new_zero(),
        }
    }

    /// Return the available balance of the `Account`
    #[inline(always)]
    pub fn available_balance(&self) -> M {
        // TODO: this call is expensive so maybe compute once and store
        let order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.maker_fee);
        let ab = self.wallet_balance - self.position.position_margin - order_margin;
        debug_assert!(ab >= M::new_zero());
        ab
    }

    /// Allows the user to update their desired leverage.
    /// This will deposit or release variation margin from the position if any.
    ///
    /// # Returns:
    /// If Err, the account is unable to provide enough variation margin for the desired leverage.
    pub fn update_desired_leverage(&mut self, _leverage: Leverage) -> Result<()> {
        todo!("Support `update_desired_leverage`")
    }

    /// Cancel an active order based on the user_order_id of an Order
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    pub(crate) fn cancel_order_by_user_id<A>(
        &mut self,
        user_order_id: u64,
        account_tracker: &mut A,
    ) -> Result<Order<M::PairedCurrency>>
    where
        A: AccountTracker<M>,
    {
        debug!("cancel_order_by_user_id: user_order_id: {}", user_order_id);
        let id: u64 = match self
            .lookup_order_nonce_from_user_order_id
            .remove(&user_order_id)
        {
            None => return Err(Error::UserOrderIdNotFound),
            Some(id) => id,
        };
        self.cancel_order(id, account_tracker)
    }

    /// Append a new limit order as active order
    pub(crate) fn append_limit_order(&mut self, order: Order<M::PairedCurrency>) {
        debug_assert!(matches!(order.order_type(), OrderType::Limit { .. }));

        debug!("append_limit_order: order: {:?}", order);

        // self.account_tracker.log_limit_order_submission();
        let order_id = order.id();
        let user_order_id = order.user_order_id();
        match self.active_limit_orders.insert(order_id, order) {
            None => {}
            Some(_) => {
                error!(
                    "there already was an order with this id in active_limit_orders. \
            This should not happen as order id should be incrementing"
                );
                debug_assert!(false)
            }
        };
        match user_order_id {
            None => {}
            Some(user_order_id) => {
                self.lookup_order_nonce_from_user_order_id
                    .insert(user_order_id, order_id);
            }
        };
        self.order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.maker_fee);
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub(crate) fn cancel_order<A>(
        &mut self,
        order_id: u64,
        account_tracker: &mut A,
    ) -> Result<Order<M::PairedCurrency>>
    where
        A: AccountTracker<M>,
    {
        debug!("cancel_order: {}", order_id);
        let removed_order = self
            .active_limit_orders
            .remove(&order_id)
            .ok_or(Error::OrderIdNotFound)?;
        self.order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.maker_fee);

        account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Removes an executed limit order from the list of active ones
    pub(crate) fn remove_executed_order_from_active(&mut self, order_id: u64) {
        let order = self
            .active_limit_orders
            .remove(&order_id)
            .expect("The order must have been active; qed");
        self.order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.maker_fee);
        if let Some(user_order_id) = order.user_order_id() {
            self.lookup_order_nonce_from_user_order_id
                .remove(&user_order_id);
        }
    }
}
