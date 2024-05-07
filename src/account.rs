use getset::{CopyGetters, Getters, MutGetters};
use hashbrown::HashMap;
use tracing::{debug, error, trace};

use crate::{
    order_margin::compute_order_margin,
    position::Position,
    prelude::AccountTracker,
    quote,
    types::{
        Currency, Error, Fee, Leverage, LimitOrder, MarginCurrency, OrderId, Pending,
        QuoteCurrency, Result, Side, TimestampNs,
    },
};

/// The users account
/// Generic over:
/// `M`: The `Currency` representing the margin currency.
/// `UserOrderId`: The type for the user defined order id.
#[derive(Debug, Clone, Getters, CopyGetters, MutGetters)]
pub struct Account<M, UserOrderId>
where
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    /// The position leverage,
    #[getset(get_copy = "pub")]
    leverage: Leverage,

    /// The wallet balance of the user denoted in the margin `Currency`.
    #[getset(get_copy = "pub")]
    available_wallet_balance: M,

    /// Get the current position of the `Account`.
    #[getset(get = "pub")]
    #[cfg_attr(test, getset(get_mut = "pub(crate)"))]
    position: Position<M>,

    /// Maps the order `id` to the actual `Order`.
    #[getset(get = "pub")]
    #[allow(clippy::type_complexity)]
    active_limit_orders:
        HashMap<OrderId, LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>>,

    // Maps the `user_order_id` to the internal order nonce.
    lookup_order_nonce_from_user_order_id: HashMap<UserOrderId, OrderId>,

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
            available_wallet_balance: M::new(Dec!(1)),
            position: Position::default(),
            active_limit_orders: HashMap::default(),
            lookup_order_nonce_from_user_order_id: HashMap::default(),
            order_margin: M::new(Dec!(0)),
            leverage: leverage!(1),
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
            available_wallet_balance: starting_balance,
            position: Position::default(),
            active_limit_orders: HashMap::new(),
            lookup_order_nonce_from_user_order_id: HashMap::new(),
            order_margin: M::new_zero(),
            leverage,
        }
    }

    /// All the account value denoted in the margin currency, which includes `wallet_balance` and position value.
    pub fn total_value(&self, bid: QuoteCurrency, ask: QuoteCurrency) -> M {
        self.available_wallet_balance + self.order_margin + self.position.value(bid, ask)
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
        let new_order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);
        let order_margin_delta = new_order_margin - self.order_margin;
        self.order_margin = new_order_margin;
        self.available_wallet_balance -= order_margin_delta;
        debug_assert!(self.available_wallet_balance >= M::new_zero());
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
        let new_order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);
        let order_margin_delta = self.order_margin - new_order_margin;
        debug_assert!(order_margin_delta > M::new_zero());
        self.order_margin = new_order_margin;
        self.available_wallet_balance += order_margin_delta;

        account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Removes an executed limit order from the list of active ones
    pub(crate) fn remove_executed_order_from_active(&mut self, order_id: OrderId) {
        let order = self
            .active_limit_orders
            .remove(&order_id)
            .expect("The order must have been active; qed");
        let new_order_margin =
            compute_order_margin(&self.position, &self.active_limit_orders, self.leverage);
        let order_margin_delta = self.order_margin - new_order_margin;
        self.order_margin = new_order_margin;
        self.available_wallet_balance += order_margin_delta;
        debug_assert!(self.available_wallet_balance >= M::new_zero());

        self.lookup_order_nonce_from_user_order_id
            .remove(order.user_order_id());
    }

    /// Detract a fee amount from `available_wallet_balance`
    pub(crate) fn detract_fee(&mut self, fee: M) {
        self.available_wallet_balance -= fee;
        debug_assert!(self.available_wallet_balance >= M::new_zero());
    }

    /// Increase a long (or neutral) position.
    ///
    /// # Arguments:
    /// `quantity`: The absolute amount to increase the position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The price at which it is sold.
    ///
    pub(crate) fn increase_long(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) {
        trace!("Account.increase_long: quantity: {quantity:?}, price: {price:?}");

        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "`quantity` must be positive"
        );
        debug_assert!(price > quote!(0), "Price must be greater than zero");

        debug_assert!(
            self.position.size >= M::PairedCurrency::new_zero(),
            "Short is open"
        );

        let new_size = self.position.size + quantity;
        trace!("new_size: {new_size}");
        debug_assert!(new_size > M::PairedCurrency::new_zero());

        self.position.entry_price = QuoteCurrency::new(
            (self.position.entry_price * self.position.size.inner() + price * quantity.inner())
                .inner()
                / new_size.inner(),
        );
        debug_assert!(self.position.entry_price > quote!(0));

        self.position.size = new_size;
        let pos_margin =
            margin_for_position(self.position.size, self.position.entry_price, self.leverage);
        let margin_delta = pos_margin - self.position.margin;
        self.position.margin = pos_margin;
        debug_assert!(self.position.margin >= M::new_zero());

        self.available_wallet_balance -= margin_delta;
        debug_assert!(self.available_wallet_balance >= M::new_zero());
    }

    /// Reduce a long position.
    ///
    /// # Arguments:
    /// `amount`: The amount to decrease the position by, must be smaller or equal to the position size.
    /// `price`: The price at which it is sold.
    ///
    pub(crate) fn decrease_long<A>(
        &mut self,
        quantity: M::PairedCurrency,
        price: QuoteCurrency,
        account_tracker: &mut A,
        ts_ns: TimestampNs,
    ) where
        A: AccountTracker<M>,
    {
        debug_assert!(quantity > M::PairedCurrency::new_zero());
        debug_assert!(price > quote!(0), "Price must be greater than zero");

        debug_assert!(
            self.position.size > M::PairedCurrency::new_zero(),
            "Open short or no position"
        );
        debug_assert!(
            quantity <= self.position.size,
            "Quantity larger than position size"
        );

        self.position.size -= quantity;
        let new_position_margin =
            margin_for_position(self.position.size, self.position.entry_price, self.leverage);
        let freed_margin = self.position.margin - new_position_margin;
        self.position.margin = new_position_margin;
        debug_assert!(self.position.margin >= M::new_zero());

        let rpnl = M::pnl(self.position.entry_price, price, quantity);
        account_tracker.log_rpnl(rpnl, ts_ns);
        self.available_wallet_balance += rpnl + freed_margin;
        assert!(self.available_wallet_balance >= M::new_zero());
    }

    /// Increase a short (or neutral) position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the short position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The entry price.
    ///
    pub(crate) fn increase_short(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) {
        trace!("Account.increase_short: quantity: {quantity:?}, price: {price:?}");

        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "Amount must be positive; qed"
        );
        debug_assert!(price > quote!(0), "Price must be greater than zero");

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
        let new_pos_margin =
            margin_for_position(self.position.size, self.position.entry_price, self.leverage);
        trace!("new_pos_margin: {new_pos_margin:?}");
        debug_assert!(new_pos_margin >= M::new_zero());

        let margin_delta = new_pos_margin - self.position.margin;
        debug_assert!(margin_delta >= M::new_zero());

        self.position.margin = new_pos_margin;
        trace!("position.margin: {:?}", self.position.margin);

        self.available_wallet_balance -= margin_delta;
        trace!(
            "available_wallet_balance: {:?}",
            self.available_wallet_balance
        );
        debug_assert!(self.available_wallet_balance >= M::new_zero());
    }

    /// Reduce a short position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to decrease the short position by.
    ///     Must be smaller or equal to the open position size.
    /// `price`: The entry price.
    ///
    pub(crate) fn decrease_short<A>(
        &mut self,
        quantity: M::PairedCurrency,
        price: QuoteCurrency,
        account_tracker: &mut A,
        ts_ns: TimestampNs,
    ) where
        A: AccountTracker<M>,
    {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "Amount must be positive; qed"
        );
        debug_assert!(price > quote!(0), "Price must be greater than zero");

        debug_assert!(
            self.position.size < M::PairedCurrency::new_zero(),
            "Position must be short!"
        );
        debug_assert!(
            quantity <= self.position.size.abs(),
            "Amount must be smaller than short position; qed"
        );

        self.position.size += quantity;
        let new_pos_margin =
            margin_for_position(self.position.size, self.position.entry_price, self.leverage);
        let margin_delta = self.position.margin - new_pos_margin;
        debug_assert!(margin_delta > M::new_zero());

        self.position.margin = new_pos_margin;
        debug_assert!(self.position.margin >= M::new_zero());

        let rpnl = M::pnl(self.position.entry_price, price, quantity.into_negative());
        account_tracker.log_rpnl(rpnl, ts_ns);

        self.available_wallet_balance += rpnl + margin_delta;
    }

    /// Settlement referes to the actual transfer of funds or assets between the buyer and seller to fulfill the trade.
    /// As the `ClearingHouse` is the central counterparty to every trade,
    /// it is the buyer of every sell order,
    /// and the seller of every buy order.
    ///
    /// # Arguments:
    /// `quantity`: The number of contract traded, where a negative number indicates a sell.
    /// `fill_price`: The execution price of the trade
    /// `fee`: The fee fraction for this type of order settlement.
    ///
    pub(crate) fn settle_filled_order<A>(
        &mut self,
        account_tracker: &mut A,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: TimestampNs,
    ) where
        A: AccountTracker<M>,
    {
        let side = if quantity > M::PairedCurrency::new_zero() {
            Side::Buy
        } else {
            Side::Sell
        };
        account_tracker.log_trade(side, fill_price, quantity);

        if quantity > M::PairedCurrency::new_zero() {
            self.settle_buy_order(account_tracker, quantity, fill_price, fee, ts_ns);
        } else {
            self.settle_sell_order(account_tracker, quantity.abs(), fill_price, fee, ts_ns);
        }
    }

    fn settle_buy_order<A>(
        &mut self,
        account_tracker: &mut A,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: TimestampNs,
    ) where
        A: AccountTracker<M>,
    {
        debug_assert!(quantity > M::PairedCurrency::new_zero());
        debug_assert!(fill_price > quote!(0));

        let notional_value = quantity.convert(fill_price);
        let fee = notional_value * fee;
        account_tracker.log_fee(fee);
        self.detract_fee(fee);

        if self.position().size() >= M::PairedCurrency::new_zero() {
            self.increase_long(quantity, fill_price);
        } else {
            // Position must be short
            if quantity.into_negative() >= self.position().size {
                // Strictly decrease the short position
                self.decrease_short(quantity, fill_price, account_tracker, ts_ns);
            } else {
                let new_long_size = quantity - self.position().size().abs();

                // decrease the short first
                self.decrease_short(
                    self.position().size().abs(),
                    fill_price,
                    account_tracker,
                    ts_ns,
                );

                // also open a long
                self.increase_long(new_long_size, fill_price);
            }
        }
    }

    fn settle_sell_order<A>(
        &mut self,
        account_tracker: &mut A,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: TimestampNs,
    ) where
        A: AccountTracker<M>,
    {
        debug_assert!(quantity > M::PairedCurrency::new_zero());
        debug_assert!(fill_price > quote!(0));

        let notional_value = quantity.convert(fill_price);
        let fee = notional_value * fee;
        account_tracker.log_fee(fee);
        self.detract_fee(fee);

        if self.position().size() > M::PairedCurrency::new_zero() {
            if quantity <= self.position().size() {
                self.decrease_long(quantity, fill_price, account_tracker, ts_ns);
            } else {
                let new_short_size = quantity - self.position().size();

                self.decrease_long(self.position().size(), fill_price, account_tracker, ts_ns);

                // Open a short as well
                self.increase_short(new_short_size, fill_price);
            }
        } else {
            // Increase short position
            self.increase_short(quantity, fill_price);
        }
    }
}

/// Compute the required margin for a position of a given size.
fn margin_for_position<Q>(
    pos_size: Q,
    entry_price: QuoteCurrency,
    leverage: Leverage,
) -> Q::PairedCurrency
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    pos_size.abs().convert(entry_price) / leverage
}
