use std::cmp::Ordering;

use getset::Getters;
use hashbrown::HashMap;
use tracing::{debug, trace};

use crate::{
    account_tracker::AccountTracker,
    accounting::TransactionAccounting,
    config::Config,
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{
        Position, Transaction, EXCHANGE_FEE_ACCOUNT, USER_ORDER_MARGIN_ACCOUNT,
        USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
    },
    quote,
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    types::{
        Currency, Error, ExchangeOrderMeta, Fee, Filled, LimitOrder, LimitOrderUpdate,
        MarginCurrency, MarketOrder, MarketUpdate, NewOrder, OrderError, OrderId, Pending, Result,
        Side, TimestampNs, UserBalances,
    },
    utils::assert_user_wallet_balance,
};

/// The datatype that holds the active limit orders of a user.
pub type ActiveLimitOrders<Q, UserOrderId> =
    HashMap<OrderId, LimitOrder<Q, UserOrderId, Pending<Q>>>;

/// Relevant information about the traders account.
pub struct Account<'a, A, Q, UserOrderId>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone,
{
    /// tracks the performance of the account
    pub account_tracker: &'a A,
    /// The active limit orders of the account.
    pub active_limit_orders: &'a ActiveLimitOrders<Q, UserOrderId>,
    /// The current position of the account.
    pub position: &'a Position<Q>,
    /// The TAccount balances of the account.
    pub balances: UserBalances<Q::PairedCurrency>,
}

/// The main leveraged futures exchange for simulated trading
#[derive(Debug, Clone, Getters)]
pub struct Exchange<A, Q, UserOrderId, TransactionAccountingT>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash + Default,
{
    /// The exchange configuration.
    #[getset(get = "pub")]
    config: Config<Q::PairedCurrency>,

    /// The current state of the simulated market.
    #[getset(get = "pub")]
    market_state: MarketState,

    /// A performance tracker for the user account.
    #[getset(get = "pub")]
    account_tracker: A,

    risk_engine: IsolatedMarginRiskEngine<Q::PairedCurrency>,

    next_order_id: u64,

    /// Does the accounting for transactions, moving balances between accounts.
    transaction_accounting: TransactionAccountingT,

    /// Get the current position of the user.
    #[getset(get = "pub")]
    #[cfg_attr(test, getset(get_mut = "pub(crate)"))]
    position: Position<Q>,

    /// Active limit orders of the user.
    /// Maps the order `id` to the actual `Order`.
    #[getset(get = "pub")]
    active_limit_orders: ActiveLimitOrders<Q, UserOrderId>,

    // Maps the `user_order_id` to the internal order nonce.
    lookup_order_nonce_from_user_order_id: HashMap<UserOrderId, OrderId>,

    order_margin: OrderMargin<Q, UserOrderId>,
}

impl<A, Q, UserOrderId, TransactionAccountingT> Exchange<A, Q, UserOrderId, TransactionAccountingT>
where
    A: AccountTracker<Q::PairedCurrency>,
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
    UserOrderId: Clone + Eq + PartialEq + std::hash::Hash + std::fmt::Debug + Default,
    TransactionAccountingT: TransactionAccounting<Q::PairedCurrency>,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<Q::PairedCurrency>) -> Self {
        let market_state = MarketState::new(config.contract_spec().price_filter().clone());
        let risk_engine =
            IsolatedMarginRiskEngine::<Q::PairedCurrency>::new(config.contract_spec().clone());

        let transaction_accounting = TransactionAccountingT::new(config.starting_wallet_balance());
        Self {
            config,
            market_state,
            risk_engine,
            account_tracker,
            next_order_id: 0,
            transaction_accounting,
            position: Position::default(),
            active_limit_orders: HashMap::default(),
            lookup_order_nonce_from_user_order_id: HashMap::default(),
            order_margin: OrderMargin::default(),
        }
    }

    /// Get information about the `Account`
    pub fn account(&self) -> Account<A, Q, UserOrderId> {
        Account {
            account_tracker: &self.account_tracker,
            active_limit_orders: &self.active_limit_orders,
            position: &self.position,
            balances: self.user_balances(),
        }
    }

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// If Ok, returns updates regarding limit orders, wether partially filled or fully.
    pub fn update_state<U>(
        &mut self,
        timestamp_ns: TimestampNs,
        market_update: U,
    ) -> Result<Vec<LimitOrderUpdate<Q, UserOrderId>>>
    where
        U: MarketUpdate<Q, UserOrderId>,
    {
        self.market_state
            .update_state(timestamp_ns, &market_update)?;
        self.account_tracker.update(
            timestamp_ns,
            &self.market_state,
            self.position()
                .unrealized_pnl(self.market_state().bid(), self.market_state().ask()),
        );
        if let Err(e) = <IsolatedMarginRiskEngine<<Q as Currency>::PairedCurrency> as RiskEngine<
            <Q as Currency>::PairedCurrency,
            UserOrderId,
        >>::check_maintenance_margin(
            &self.risk_engine, &self.market_state, &self.position
        ) {
            // TODO: liquidate position properly
            return Err(e.into());
        };

        Ok(self.check_active_orders(market_update, timestamp_ns))
    }

    /// Submit a new `MarketOrder` to the exchange.
    ///
    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_market_order(
        &mut self,
        order: MarketOrder<Q, UserOrderId, NewOrder>,
    ) -> Result<MarketOrder<Q, UserOrderId, Filled>> {
        // Basic checks
        self.config
            .contract_spec()
            .quantity_filter()
            .validate_order_quantity(order.quantity())?;

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);

        let fill_price = match order.side() {
            Side::Buy => self.market_state.ask(),
            Side::Sell => self.market_state.bid(),
        };
        let position_margin = self
            .transaction_accounting
            .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)?;
        let available_wallet_balance = self
            .transaction_accounting
            .margin_balance_of(USER_WALLET_ACCOUNT)?;
        self.risk_engine.check_market_order(
            &self.position,
            position_margin,
            &order,
            fill_price,
            available_wallet_balance,
        )?;

        // From here on, everything is infallible
        let filled_order = order.into_filled(fill_price, self.market_state.current_timestamp_ns());
        self.settle_filled_market_order(filled_order.clone());

        Ok(filled_order)
    }

    fn settle_filled_market_order(&mut self, order: MarketOrder<Q, UserOrderId, Filled>) {
        let filled_qty = order.quantity();
        assert!(filled_qty > Q::new_zero());
        let fill_price = order.state().avg_fill_price();
        assert!(fill_price > quote!(0));
        Self::detract_fee(
            &mut self.transaction_accounting,
            filled_qty.convert(fill_price),
            self.config.contract_spec().fee_taker(),
        );

        self.position.change_position(
            filled_qty,
            fill_price,
            order.side(),
            &mut self.transaction_accounting,
            self.config.contract_spec().init_margin_req(),
        );
        self.account_tracker
            .log_trade(order.side(), fill_price, filled_qty);
    }

    #[inline]
    fn next_order_id(&mut self) -> OrderId {
        self.next_order_id += 1;
        self.next_order_id - 1
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
    pub fn cancel_order_by_user_id(
        &mut self,
        user_order_id: UserOrderId,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending<Q>>> {
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
        self.cancel_limit_order(id)
    }

    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_limit_order(
        &mut self,
        order: LimitOrder<Q, UserOrderId, NewOrder>,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending<Q>>> {
        trace!("submit_order: {:?}", order);

        // Basic checks
        self.config
            .contract_spec()
            .quantity_filter()
            .validate_order_quantity(order.remaining_quantity())?;
        self.config
            .contract_spec()
            .price_filter()
            .validate_limit_order(&order, self.market_state.mid_price())?;

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);

        match order.side() {
            Side::Buy => {
                if order.limit_price() >= self.market_state.ask() {
                    return Err(Error::OrderError(OrderError::LimitPriceAboveAsk));
                }
            }
            Side::Sell => {
                if order.limit_price() <= self.market_state.bid() {
                    return Err(Error::OrderError(OrderError::LimitPriceBelowBid));
                }
            }
        }
        let available_wallet_balance = self
            .transaction_accounting
            .margin_balance_of(USER_WALLET_ACCOUNT)?;
        self.risk_engine.check_limit_order(
            &self.position,
            &order,
            available_wallet_balance,
            &self.order_margin,
        )?;
        self.append_limit_order(order.clone());
        self.account_tracker.log_limit_order_submission();

        Ok(order)
    }

    /// Append a new limit order as active order
    fn append_limit_order(&mut self, order: LimitOrder<Q, UserOrderId, Pending<Q>>) {
        debug!("append_limit_order: order: {:?}", order);

        let order_id = order.id();
        let user_order_id = order.user_order_id().clone();
        self.order_margin
            .update(&order, self.config.contract_spec().fee_maker());
        self.active_limit_orders.insert(order_id, order);
        self.lookup_order_nonce_from_user_order_id
            .insert(user_order_id, order_id);
        let new_order_margin = self.order_margin.order_margin(
            self.config.contract_spec().init_margin_req(),
            &self.position,
        );
        let order_margin = self
            .transaction_accounting
            .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
            .expect("Is valid");

        let transaction = match new_order_margin.cmp(&order_margin) {
            Ordering::Greater => Transaction::new(
                USER_ORDER_MARGIN_ACCOUNT,
                USER_WALLET_ACCOUNT,
                new_order_margin - order_margin,
            ),
            Ordering::Less => Transaction::new(
                USER_ORDER_MARGIN_ACCOUNT,
                USER_WALLET_ACCOUNT,
                order_margin - new_order_margin,
            ),
            Ordering::Equal => return,
        };

        self.transaction_accounting
            .create_margin_transfer(transaction)
            .expect("margin transfer works");
        assert_user_wallet_balance(&self.transaction_accounting);
    }

    /// Cancel an active limit order.
    /// returns Some order if successful with given order_id
    pub fn cancel_limit_order(
        &mut self,
        order_id: OrderId,
    ) -> Result<LimitOrder<Q, UserOrderId, Pending<Q>>> {
        debug!("cancel_order: {}", order_id);
        let removed_order = self
            .active_limit_orders
            .remove(&order_id)
            .ok_or(Error::OrderIdNotFound)?;
        let order_margin = self
            .transaction_accounting
            .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
            .expect("is valid");
        assert_eq!(
            order_margin,
            self.order_margin.order_margin(
                self.config.contract_spec().init_margin_req(),
                &self.position,
            )
        );
        let new_order_margin = self.order_margin.order_margin(
            self.config.contract_spec().init_margin_req(),
            &self.position,
        );

        assert!(new_order_margin <= order_margin, "When cancelling a limit order, the new order margin is smaller or equal the old order margin");
        if new_order_margin < order_margin {
            let delta = order_margin - new_order_margin;
            let transaction =
                Transaction::new(USER_WALLET_ACCOUNT, USER_ORDER_MARGIN_ACCOUNT, delta);
            self.transaction_accounting
                .create_margin_transfer(transaction)
                .expect("margin transfer works.");
        }

        self.account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Removes an executed limit order from the list of active ones.
    pub(crate) fn remove_executed_order_from_active(&mut self, order_id: OrderId) {
        let order = self
            .active_limit_orders
            .remove(&order_id)
            .expect("The order must have been active; qed");
        self.lookup_order_nonce_from_user_order_id
            .remove(order.user_order_id());
    }

    /// Remove a fee amount from the wallet balance.
    fn detract_fee(
        transaction_accounting: &mut TransactionAccountingT,
        trade_value: Q::PairedCurrency,
        fee: Fee,
    ) {
        assert!(trade_value > Q::PairedCurrency::new_zero());

        let fee: Q::PairedCurrency = trade_value * fee;
        trace!("detract_fee: {fee}");
        let transaction = Transaction::new(EXCHANGE_FEE_ACCOUNT, USER_WALLET_ACCOUNT, fee);
        transaction_accounting
            .create_margin_transfer(transaction)
            .expect("margin transfer works");

        assert_user_wallet_balance(transaction_accounting);
    }

    /// Checks for the execution of active limit orders in the account.
    fn check_active_orders<U>(
        &mut self,
        market_update: U,
        ts_ns: TimestampNs,
    ) -> Vec<LimitOrderUpdate<Q, UserOrderId>>
    where
        U: MarketUpdate<Q, UserOrderId>,
    {
        let mut order_updates = Vec::new();
        let mut ids_to_remove = Vec::new();
        for order in self.active_limit_orders.values_mut() {
            if let Some(filled_qty) = market_update.limit_order_filled(order) {
                trace!(
                    "filled order {}: {filled_qty}/{}",
                    order.id(),
                    order.total_quantity()
                );

                let order_margin = self
                    .transaction_accounting
                    .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                    .expect("is valid");

                if let Some(filled_order) = order.fill(filled_qty, ts_ns) {
                    trace!("fully filled order {}", order.id());
                    order_updates.push(LimitOrderUpdate::FullyFilled(filled_order));

                    ids_to_remove.push(order.state().meta().id());
                    self.account_tracker.log_limit_order_fill();
                    self.order_margin
                        .update(order, self.config.contract_spec().fee_maker());
                    self.order_margin
                        .remove_order(order, self.config.contract_spec().fee_maker());
                } else {
                    order_updates.push(LimitOrderUpdate::PartiallyFilled(order.clone()));
                    self.order_margin
                        .update(order, self.config.contract_spec().fee_maker());
                    // TODO: we could potentially log partial fills as well...
                }

                Self::detract_fee(
                    &mut self.transaction_accounting,
                    filled_qty.convert(order.limit_price()),
                    self.config.contract_spec().fee_maker(),
                );

                let new_order_margin = self.order_margin.order_margin(
                    self.config.contract_spec().init_margin_req(),
                    &self.position,
                );
                trace!("order_margin: {order_margin}, new_order_margin: {new_order_margin}");
                assert!(
                    new_order_margin <= order_margin,
                    "The order margin does not increase with a filled limit order event."
                );

                if new_order_margin < order_margin {
                    let delta = order_margin - new_order_margin;
                    let transaction =
                        Transaction::new(USER_WALLET_ACCOUNT, USER_ORDER_MARGIN_ACCOUNT, delta);
                    self.transaction_accounting
                        .create_margin_transfer(transaction)
                        .expect("margin transfer works");
                }
                assert_user_wallet_balance(&self.transaction_accounting);

                self.position.change_position(
                    filled_qty,
                    order.limit_price(),
                    order.side(),
                    &mut self.transaction_accounting,
                    self.config.contract_spec().init_margin_req(),
                );
            }
        }
        ids_to_remove
            .into_iter()
            .for_each(|id| self.remove_executed_order_from_active(id));

        order_updates
    }

    /// Get the balances of the user account.
    pub fn user_balances(&self) -> UserBalances<Q::PairedCurrency> {
        UserBalances {
            available_wallet_balance: self
                .transaction_accounting
                .margin_balance_of(USER_WALLET_ACCOUNT)
                .expect("is a valid account"),
            position_margin: self
                .transaction_accounting
                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                .expect("is a valid account"),
            order_margin: self
                .transaction_accounting
                .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                .expect("is a valid account"),
        }
    }
}
