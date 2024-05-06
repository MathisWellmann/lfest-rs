use getset::{CopyGetters, Getters};
use hashbrown::HashMap;
use tracing::{debug, error};

use crate::{
    order_margin::compute_order_margin,
    position::Position,
    prelude::AccountTracker,
    quote,
    types::{
        Currency, Error, Leverage, LimitOrder, MarginCurrency, OrderId, Pending, QuoteCurrency,
        Result,
    },
};

/// The users account
/// Generic over:
/// `M`: The `Currency` representing the margin currency.
/// `UserOrderId`: The type for the user defined order id.
#[derive(Debug, Clone, Getters, CopyGetters)]
pub struct Account<M, UserOrderId>
where
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    /// The position leverage,
    #[getset(get_copy = "pub")]
    leverage: Leverage,

    /// The wallet balance of the user denoted in the margin currency.
    #[getset(get_copy = "pub")]
    pub(crate) wallet_balance: M,

    /// Get the current position of the `Account`.
    #[getset(get = "pub")]
    pub(crate) position: Position<M>,

    /// Maps the order `id` to the actual `Order`.
    #[getset(get = "pub")]
    #[allow(clippy::type_complexity)]
    pub(crate) active_limit_orders:
        HashMap<OrderId, LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>>,

    // Maps the `user_order_id` to the internal order nonce.
    pub(crate) lookup_order_nonce_from_user_order_id: HashMap<UserOrderId, OrderId>,

    /// The current order margin used by the user `Account`.
    #[getset(get_copy = "pub")]
    order_margin: M,
}

#[cfg(test)]
impl<M, UserOrderId> Default for Account<M, UserOrderId>
where
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    fn default() -> Self {
        use crate::prelude::{leverage, Dec, Decimal};
        Self {
            leverage: leverage!(1),
            wallet_balance: M::new(Dec!(1)),
            position: Position::default(),
            active_limit_orders: HashMap::default(),
            lookup_order_nonce_from_user_order_id: HashMap::default(),
            order_margin: M::new(Dec!(0)),
        }
    }
}

impl<M, UserOrderId> Account<M, UserOrderId>
where
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: M, leverage: Leverage) -> Self {
        Self {
            leverage,
            wallet_balance: starting_balance,
            position: Position::default(),
            active_limit_orders: HashMap::new(),
            lookup_order_nonce_from_user_order_id: HashMap::new(),
            order_margin: M::new_zero(),
        }
    }

    /// Return the available balance of the `Account`
    pub fn available_balance(&self) -> M {
        // TODO: this call is expensive so maybe compute once and store
        let order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);
        let ab = self.wallet_balance - self.position.margin - order_margin;
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

    /// Cancel an active limit order based on the `user_order_id`.
    ///
    /// # Arguments:
    /// `user_order_id`: The order id from the user.
    /// `account_tracker`: Something to track this action.
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    pub(crate) fn cancel_order_by_user_id<A>(
        &mut self,
        user_order_id: UserOrderId,
        account_tracker: &mut A,
    ) -> Result<LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>>
    where
        A: AccountTracker<M>,
    {
        debug!(
            "cancel_order_by_user_id: user_order_id: {:?}",
            user_order_id
        );
        let id: u64 = match self
            .lookup_order_nonce_from_user_order_id
            .remove(&user_order_id)
        {
            None => return Err(Error::UserOrderIdNotFound),
            Some(id) => id,
        };
        self.cancel_limit_order(id, account_tracker)
    }

    /// Append a new limit order as active order
    pub(crate) fn append_limit_order(
        &mut self,
        order: LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
    ) {
        debug!("append_limit_order: order: {:?}", order);

        let order_id = order.state().meta().id();
        let user_order_id = order.user_order_id().clone();
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
        self.lookup_order_nonce_from_user_order_id
            .insert(user_order_id, order_id);
        self.order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);
    }

    /// Cancel an active limit order.
    /// returns Some order if successful with given order_id
    pub(crate) fn cancel_limit_order<A>(
        &mut self,
        order_id: OrderId,
        account_tracker: &mut A,
    ) -> Result<LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>>
    where
        A: AccountTracker<M>,
    {
        debug!("cancel_order: {}", order_id);
        let removed_order = self
            .active_limit_orders
            .remove(&order_id)
            .ok_or(Error::OrderIdNotFound)?;
        self.order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);

        account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Removes an executed limit order from the list of active ones
    pub(crate) fn remove_executed_order_from_active(&mut self, order_id: OrderId) {
        let order = self
            .active_limit_orders
            .remove(&order_id)
            .expect("The order must have been active; qed");
        self.order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);
        self.lookup_order_nonce_from_user_order_id
            .remove(order.user_order_id());
    }

    /// Create a new position with all fields custom.
    ///
    /// # Arguments:
    /// `size`: The position size, negative denoting a negative position.
    ///     The `size` must have been approved by the `RiskEngine`.
    /// `entry_price`: The price at which the position was entered.
    ///
    pub(crate) fn open_position(&mut self, size: M::PairedCurrency, price: QuoteCurrency) {
        debug_assert!(price > quote!(0), "Price must be greater than zero");

        self.position.size = size;
        self.position.entry_price = price;
        self.position.margin =
            self.position.size.abs().convert(self.position.entry_price) / self.leverage;
    }

    /// Increase a long (or neutral) position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The price at which it is sold.
    ///
    pub(crate) fn increase_long(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "`amount` must be positive"
        );
        debug_assert!(
            self.position.size >= M::PairedCurrency::new_zero(),
            "Short is open"
        );

        let new_size = self.position.size + quantity;
        self.position.entry_price = QuoteCurrency::new(
            (self.position.entry_price * self.position.size.inner() + price * quantity.inner())
                .inner()
                / new_size.inner(),
        );

        self.position.size = new_size;
        self.position.margin =
            self.position.size.abs().convert(self.position.entry_price) / self.leverage;
    }

    /// Reduce a long position.
    ///
    /// # Arguments:
    /// `amount`: The amount to decrease the position by, must be smaller or equal to the position size.
    /// `price`: The price at which it is sold.
    ///
    /// # Returns:
    /// If Ok, the net realized profit and loss for that specific futures contract.
    #[must_use]
    pub(crate) fn decrease_long(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) -> M {
        debug_assert!(
            self.position.size > M::PairedCurrency::new_zero(),
            "Open short or no position"
        );
        debug_assert!(quantity > M::PairedCurrency::new_zero());
        debug_assert!(
            quantity <= self.position.size,
            "Quantity larger than position size"
        );
        self.position.size -= quantity;
        self.position.margin =
            self.position.size.abs().convert(self.position.entry_price) / self.leverage;

        M::pnl(self.position.entry_price, price, quantity)
    }

    /// Increase a short position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the short position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The entry price.
    ///
    pub(crate) fn increase_short(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "Amount must be positive; qed"
        );
        debug_assert!(
            self.position.size <= M::PairedCurrency::new_zero(),
            "Position must not be long; qed"
        );
        let new_size = self.position.size - quantity;
        self.position.entry_price = QuoteCurrency::new(
            (self.position.entry_price.inner() * self.position.size.inner().abs()
                + price.inner() * quantity.inner())
                / new_size.inner().abs(),
        );
        self.position.size = new_size;
        self.position.margin =
            self.position.size.abs().convert(self.position.entry_price) / self.leverage;
    }

    /// Reduce a short position
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to decrease the short position by.
    ///     Must be smaller or equal to the open position size.
    /// `price`: The entry price.
    ///
    /// # Returns:
    /// If Ok, the net realized profit and loss for that specific futures contract.
    pub(crate) fn decrease_short(
        &mut self,
        quantity: M::PairedCurrency,
        price: QuoteCurrency,
    ) -> M {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "Amount must be positive; qed"
        );
        debug_assert!(
            self.position.size < M::PairedCurrency::new_zero(),
            "Position must be short!"
        );
        debug_assert!(
            quantity <= self.position.size.abs(),
            "Amount must be smaller than short position; qed"
        );

        self.position.size += quantity;
        self.position.margin =
            self.position.size.abs().convert(self.position.entry_price) / self.leverage;

        M::pnl(self.position.entry_price, price, quantity.into_negative())
    }
}
