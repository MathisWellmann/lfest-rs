use std::cmp::Ordering;

use assert2::assert;
use getset::Getters;
use num_traits::Zero;
use tracing::{debug, info, trace, warn};

use crate::{
    account_tracker::AccountTracker,
    accounting::TransactionAccounting,
    config::Config,
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{
        ActiveLimitOrders, Currency, MarketUpdate, Mon, OrderError, Position, QuoteCurrency,
        RePricing, Transaction, EXCHANGE_FEE_ACCOUNT, USER_ORDER_MARGIN_ACCOUNT,
        USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
    },
    risk_engine::{IsolatedMarginRiskEngine, RiskEngine},
    sample_returns_trigger::SampleReturnsTrigger,
    types::{
        Error, ExchangeOrderMeta, Filled, LimitOrder, LimitOrderUpdate, MarginCurrency,
        MarketOrder, NewOrder, OrderId, Pending, Result, Side, TimestampNs, UserBalances,
        UserOrderIdT,
    },
    utils::assert_user_wallet_balance,
};

/// Whether to cancel a limit order by its `OrderId` or the `UserOrderId`.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy)]
pub enum CancelBy<UserOrderId: UserOrderIdT> {
    OrderId(OrderId),
    UserOrderId(UserOrderId),
}

/// Relevant information about the traders account.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
/// - `A`: An `AccountTracker`
pub struct Account<'a, I, const D: u8, BaseOrQuote, UserOrderId, A>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderId: UserOrderIdT,
{
    /// tracks the performance of the account
    pub account_tracker: &'a A,
    /// The active limit orders of the account.
    pub active_limit_orders: &'a ActiveLimitOrders<I, D, BaseOrQuote, UserOrderId>,
    /// The current position of the account.
    pub position: &'a Position<I, D, BaseOrQuote>,
    /// The TAccount balances of the account.
    pub balances: UserBalances<I, D, BaseOrQuote::PairedCurrency>,
}

/// The main leveraged futures exchange for simulated trading
#[derive(Debug, Clone, Getters)]
pub struct Exchange<I, const D: u8, BaseOrQuote, UserOrderId, TransactionAccountingT, A>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderId: UserOrderIdT,
{
    /// The exchange configuration.
    #[getset(get = "pub")]
    config: Config<I, D, BaseOrQuote::PairedCurrency>,

    /// The current state of the simulated market.
    #[getset(get = "pub")]
    market_state: MarketState<I, D>,

    /// A performance tracker for the user account.
    #[getset(get = "pub")]
    account_tracker: A,

    risk_engine: IsolatedMarginRiskEngine<I, D, BaseOrQuote>,

    next_order_id: OrderId,

    /// Does the accounting for transactions, moving balances between accounts.
    transaction_accounting: TransactionAccountingT,

    /// Get the current position of the user.
    #[getset(get = "pub")]
    #[cfg_attr(test, getset(get_mut = "pub(crate)"))]
    position: Position<I, D, BaseOrQuote>,

    /// Active limit orders of the user.
    /// Maps the order `id` to the actual `Order`.
    #[getset(get = "pub")]
    active_limit_orders: ActiveLimitOrders<I, D, BaseOrQuote, UserOrderId>,

    order_margin: OrderMargin<I, D, BaseOrQuote, UserOrderId>,

    sample_returns_trigger: SampleReturnsTrigger,

    // To avoid allocations in hot-paths
    limit_order_updates: Vec<LimitOrderUpdate<I, D, BaseOrQuote, UserOrderId>>,
    ids_to_remove: Vec<OrderId>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderId, TransactionAccountingT, A>
    Exchange<I, D, BaseOrQuote, UserOrderId, TransactionAccountingT, A>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    A: AccountTracker<I, D, BaseOrQuote::PairedCurrency>,
    UserOrderId: UserOrderIdT,
    TransactionAccountingT:
        TransactionAccounting<I, D, BaseOrQuote::PairedCurrency> + std::fmt::Debug,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<I, D, BaseOrQuote::PairedCurrency>) -> Self {
        let market_state = MarketState::default();
        let risk_engine = IsolatedMarginRiskEngine::new(config.contract_spec().clone());

        let transaction_accounting = TransactionAccountingT::new(config.starting_wallet_balance());
        let sample_returns_trigger = SampleReturnsTrigger::new(Into::<TimestampNs>::into(
            config.sample_returns_every_n_seconds() as i64 * 1_000_000_000,
        ));
        let max_active_orders = config.max_num_open_orders();
        Self {
            config,
            market_state,
            risk_engine,
            account_tracker,
            next_order_id: OrderId::default(),
            transaction_accounting,
            position: Position::default(),
            // TODO: two such structs, one for buys, the other for sells.
            active_limit_orders: ActiveLimitOrders::new(10_000),
            order_margin: OrderMargin::new(max_active_orders),
            sample_returns_trigger,
            limit_order_updates: Vec::with_capacity(max_active_orders),
            ids_to_remove: Vec::with_capacity(max_active_orders),
        }
    }

    /// Get information about the `Account`
    pub fn account(&self) -> Account<I, D, BaseOrQuote, UserOrderId, A> {
        Account {
            account_tracker: &self.account_tracker,
            active_limit_orders: &self.active_limit_orders,
            position: &self.position,
            balances: self.user_balances(),
        }
    }

    /// Get the total amount of fees paid to the exchange.
    pub fn fees_paid(&self) -> BaseOrQuote::PairedCurrency {
        self.transaction_accounting
            .margin_balance_of(EXCHANGE_FEE_ACCOUNT)
            .expect("is valid account")
    }

    /// Update the exchange state with new information
    /// Returns a reference to order updates vector for performance reasons.
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
        market_update: &U,
    ) -> Result<&Vec<LimitOrderUpdate<I, D, BaseOrQuote, UserOrderId>>>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        trace!("update_state: market_update: {market_update}");

        self.market_state
            .update_state(market_update, self.config.contract_spec().price_filter())?;

        self.account_tracker.update(&self.market_state);
        if self
            .sample_returns_trigger
            .should_trigger(market_update.timestamp_exchange_ns())
        {
            self.account_tracker
                .sample_user_balances(&self.user_balances(), self.market_state.mid_price());
        }

        if let Err(e) = <IsolatedMarginRiskEngine<I, D, BaseOrQuote> as RiskEngine<
            I,
            D,
            BaseOrQuote,
            UserOrderId,
        >>::check_maintenance_margin(
            &self.risk_engine, &self.market_state, &self.position
        ) {
            self.liquidate();
            return Err(e.into());
        };

        self.check_active_orders(market_update);
        Ok(&self.limit_order_updates)
    }

    // Liquidate the position by closing it with a market order.
    fn liquidate(&mut self) {
        warn!("liquidating position {}", self.position);
        let order = match &self.position {
            Position::Long(pos) => {
                MarketOrder::new(Side::Sell, pos.quantity()).expect("Can create market order.")
            }
            Position::Short(pos) => {
                MarketOrder::new(Side::Buy, pos.quantity()).expect("Can create market order.")
            }
            Position::Neutral => panic!("A neutral position can not be liquidated"),
        };
        self.submit_market_order(order)
            .expect("Must be able to submit liquidation order");
        info!("balances after liquidation: {:?}", self.user_balances());
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
        order: MarketOrder<I, D, BaseOrQuote, UserOrderId, NewOrder>,
    ) -> Result<MarketOrder<I, D, BaseOrQuote, UserOrderId, Filled<I, D, BaseOrQuote>>> {
        self.account_tracker.log_market_order_submission();

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

        let filled_order = order.into_filled(fill_price, self.market_state.current_timestamp_ns());
        self.settle_filled_market_order(filled_order.clone());

        Ok(filled_order)
    }

    fn settle_filled_market_order(
        &mut self,
        order: MarketOrder<I, D, BaseOrQuote, UserOrderId, Filled<I, D, BaseOrQuote>>,
    ) {
        let filled_qty = order.quantity();
        assert!(filled_qty > BaseOrQuote::zero());
        let fill_price = order.state().avg_fill_price();
        assert!(fill_price > QuoteCurrency::zero());

        let value = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price);
        let fees = value * *self.config.contract_spec().fee_taker().as_ref();

        self.position.change_position(
            filled_qty,
            fill_price,
            order.side(),
            &mut self.transaction_accounting,
            self.config.contract_spec().init_margin_req(),
            fees,
        );
        self.account_tracker.log_market_order_fill();
        self.account_tracker
            .log_trade(order.side(), fill_price, filled_qty);
    }

    #[inline]
    fn next_order_id(&mut self) -> OrderId {
        let oid = self.next_order_id;
        self.next_order_id.incr();
        oid
    }

    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the order with timestamp and id filled in.
    /// Else its an error.
    pub fn submit_limit_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderId, NewOrder>,
    ) -> Result<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        trace!("submit_order: {}", order);

        // Basic checks
        self.config
            .contract_spec()
            .quantity_filter()
            .validate_order_quantity(order.remaining_quantity())?;
        self.config
            .contract_spec()
            .price_filter()
            .validate_limit_price(order.limit_price(), self.market_state.mid_price())?;

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);

        let available_wallet_balance = self
            .transaction_accounting
            .margin_balance_of(USER_WALLET_ACCOUNT)?;
        self.risk_engine.check_limit_order(
            &self.position,
            &order,
            available_wallet_balance,
            &self.order_margin,
        )?;

        // If a limit order is marketable, it will take liquidity from the book at the `limit_price` price level and pay the taker fee,
        let marketable = match order.side() {
            Side::Buy => order.limit_price() >= self.market_state.ask(),
            Side::Sell => order.limit_price() <= self.market_state.bid(),
        };
        match order.re_pricing() {
            RePricing::GoodTilCrossing => {
                if marketable {
                    return Err(Error::OrderError(
                        OrderError::GoodTillCrossingRejectedOrder {
                            limit_price: order.limit_price().to_string(),
                            away_market_quotation_price: match order.side() {
                                Side::Buy => self.market_state.ask().to_string(),
                                Side::Sell => self.market_state.bid().to_string(),
                            },
                        },
                    ));
                }
            }
        }

        self.append_limit_order(order.clone(), marketable)?;
        self.account_tracker.log_limit_order_submission();

        Ok(order)
    }

    /// Amend an existing limit order.
    ///
    /// The amend message will only be accepted if the original order can be successfully removed.
    /// Requests which cannot be processed will be rejected with an error.
    ///
    /// The new order get a new `OrderId` as well.
    pub fn amend_limit_order(
        &mut self,
        existing_order_id: OrderId,
        mut new_order: LimitOrder<I, D, BaseOrQuote, UserOrderId, NewOrder>,
    ) -> Result<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        let existing_order = self
            .active_limit_orders
            .get_by_id(existing_order_id)
            .ok_or_else(|| {
                if existing_order_id < self.next_order_id {
                    Error::OrderNoLongerActive
                } else {
                    Error::OrderIdNotFound {
                        order_id: existing_order_id,
                    }
                }
            })?;
        // When the order is in partially filled status and the new quantity <= `filled_quantity`, as per `binance` docs.
        //
        // As per cboe: "Changes in OrderQty result in an adjustment of the current order’s OrderQty. The new OrderQty does
        // not directly replace the current order’s LeavesQty. Rather, a delta is computed from the current
        // OrderQty and the replacement OrderQty. This delta is then applied to the current LeavesQty. If the
        // resulting LeavesQty is less than or equal to zero, the order is cancelled. This results in safer behavior
        // when the modification request overlaps partial fills for the current order, leaving the Member in total
        // control of the share exposure of the order"
        let qty_delta = new_order.total_quantity() - existing_order.total_quantity();
        trace!("qty_delta: {qty_delta}");
        let new_leaves_qty = existing_order.remaining_quantity() + qty_delta;
        if new_leaves_qty <= BaseOrQuote::zero() {
            self.cancel_limit_order(CancelBy::OrderId(existing_order_id))
                .expect("Can cancel this order");
            return Err(Error::AmendQtyAlreadyFilled);
        }

        new_order.set_remaining_quantity(new_leaves_qty);

        self.cancel_limit_order(CancelBy::OrderId(existing_order_id))?;
        self.submit_limit_order(new_order)
    }

    /// Append a new limit order as active order.
    /// If limit order is `marketable`, the order will take liquidity from the book at the `limit_price` price level.
    /// Then it pays the taker fee for the quantity that was taken from the book, the rest of the quantity (if any)
    /// will be placed into the book as a passive order.
    fn append_limit_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        marketable: bool,
    ) -> Result<()> {
        debug!("append_limit_order: order: {order}, marketable: {marketable}");
        debug!(
            "active_limit_orders: {}, market_state: {}, position: {}",
            self.active_limit_orders, self.market_state, self.position,
        );

        self.order_margin.update(&order)?;
        self.active_limit_orders.insert(order)?;
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
            Ordering::Equal => return Ok(()),
        };
        self.transaction_accounting
            .create_margin_transfer(transaction)
            .expect("margin transfer works");

        assert_eq!(
            self.order_margin.active_limit_orders(),
            &self.active_limit_orders
        );
        assert!(if self.active_limit_orders.is_empty() {
            self.transaction_accounting
                .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                .expect("is a valid account")
                .is_zero()
        } else {
            true
        });
        assert_user_wallet_balance(&self.transaction_accounting);

        Ok(())
    }

    /// Cancel an active limit order.
    /// returns Some order if successful with given order_id
    pub fn cancel_limit_order(
        &mut self,
        cancel_by: CancelBy<UserOrderId>,
    ) -> Result<LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>> {
        trace!("cancel_order: by {:?}", cancel_by);
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
        let removed_order = match cancel_by {
            CancelBy::OrderId(order_id) => self
                .active_limit_orders
                .remove_by_order_id(order_id)
                .ok_or_else(|| {
                    if order_id < self.next_order_id {
                        Error::OrderNoLongerActive
                    } else {
                        Error::OrderIdNotFound { order_id }
                    }
                })?,
            CancelBy::UserOrderId(user_order_id) => self
                .active_limit_orders
                .remove_by_user_order_id(user_order_id)
                .ok_or(Error::UserOrderIdNotFound)?,
        };
        self.order_margin.remove(cancel_by);

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

        assert_eq!(
            self.order_margin.active_limit_orders(),
            &self.active_limit_orders
        );
        assert!(if self.active_limit_orders.is_empty() {
            self.transaction_accounting
                .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                .expect("is a valid account")
                .is_zero()
        } else {
            true
        });

        Ok(removed_order)
    }

    /// Removes an executed limit order from the list of active ones.
    /// order margin updates are handled separately.
    #[inline]
    pub(crate) fn remove_executed_order_from_active(
        order_id: OrderId,
        active_limit_orders: &mut ActiveLimitOrders<I, D, BaseOrQuote, UserOrderId>,
    ) {
        let order = active_limit_orders
            .remove_by_order_id(order_id)
            .expect("The order must have been active; qed");
        debug_assert_eq!(order.id(), order_id);
    }

    /// Checks for the execution of active limit orders in the account.
    /// NOTE: only public for benchmarking purposes.
    pub fn check_active_orders<U>(&mut self, market_update: &U)
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        // Clear any potential order updates from the previous iteration.
        self.limit_order_updates.clear();

        if !U::CAN_FILL_LIMIT_ORDERS {
            return;
        }

        debug_assert_eq!(
            self.order_margin.active_limit_orders(),
            &self.active_limit_orders
        );

        for order in self.active_limit_orders.values_mut() {
            if let Some(filled_qty) = market_update.limit_order_filled(order) {
                trace!(
                    "filled limit {} order {}: {filled_qty}/{} @ {}",
                    order.side(),
                    order.id(),
                    order.remaining_quantity(),
                    order.limit_price()
                );
                debug_assert!(
                    filled_qty > BaseOrQuote::zero(),
                    "The filled_qty must be greater than zero"
                );

                let order_margin = self
                    .transaction_accounting
                    .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                    .expect("is valid");
                debug_assert_eq!(
                    order_margin,
                    self.order_margin.order_margin(
                        self.config.contract_spec().init_margin_req(),
                        &self.position
                    )
                );

                if let Some(filled_order) =
                    order.fill(filled_qty, market_update.timestamp_exchange_ns())
                {
                    self.ids_to_remove.push(order.state().meta().id());
                    self.account_tracker.log_limit_order_fill(true);
                    self.order_margin.remove(CancelBy::OrderId(order.id()));
                    self.limit_order_updates
                        .push(LimitOrderUpdate::FullyFilled(filled_order));
                } else {
                    debug_assert!(order.remaining_quantity() > BaseOrQuote::zero());
                    self.account_tracker.log_limit_order_fill(false);
                    self.limit_order_updates
                        .push(LimitOrderUpdate::PartiallyFilled(order.clone()));
                    self.order_margin
                        .update(order)
                        .expect("Can update an existing order");
                }

                let value =
                    BaseOrQuote::PairedCurrency::convert_from(filled_qty, order.limit_price());
                let fees = value * *self.config.contract_spec().fee_maker().as_ref();
                self.position.change_position(
                    filled_qty,
                    order.limit_price(),
                    order.side(),
                    &mut self.transaction_accounting,
                    self.config.contract_spec().init_margin_req(),
                    fees,
                );
                self.account_tracker
                    .log_trade(order.side(), order.limit_price(), filled_qty);

                let new_order_margin = self.order_margin.order_margin(
                    self.config.contract_spec().init_margin_req(),
                    &self.position,
                );
                debug_assert!(
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
            }
        }
        self.ids_to_remove.iter().for_each(|id| {
            Self::remove_executed_order_from_active(*id, &mut self.active_limit_orders)
        });
        self.ids_to_remove.clear();
        debug_assert_eq!(
            self.ids_to_remove.capacity(),
            self.config.max_num_open_orders()
        );

        debug_assert_eq!(
            self.order_margin.active_limit_orders(),
            &self.active_limit_orders
        );
        debug_assert!(if self.active_limit_orders.is_empty() {
            self.transaction_accounting
                .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                .expect("is a valid account")
                .is_zero()
        } else {
            true
        });
        debug_assert_eq!(
            self.transaction_accounting
                .margin_balance_of(USER_ORDER_MARGIN_ACCOUNT)
                .expect("is valid"),
            self.order_margin.order_margin(
                self.config.contract_spec().init_margin_req(),
                &self.position
            )
        );
        assert_user_wallet_balance(&self.transaction_accounting);
    }

    /// Get the balances of the user account.
    #[inline]
    pub fn user_balances(&self) -> UserBalances<I, D, BaseOrQuote::PairedCurrency> {
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
            _q: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    // use fpdec::Dec;

    // use super::*;
    // use crate::{
    //     base,
    //     contract_specification::ContractSpecification,
    //     fee, leverage,
    //     prelude::{
    //         BaseCurrency, Decimal, InMemoryTransactionAccounting, NoAccountTracker, PositionInner,
    //         PriceFilter, QuantityFilter, QuoteCurrency, TAccount,
    //     },
    // };

    // #[test]
    // #[tracing_test::traced_test]
    // fn some_debugging() {
    //     let contract_spec = ContractSpecification::new(
    //         leverage!(1),
    //         Dec!(0.5),
    //         PriceFilter::new(None, None, quote!(0.1), Dec!(2), Dec!(0.5)).unwrap(),
    //         QuantityFilter::new(None, None, base!(0.001)).unwrap(),
    //         fee!(0.0002),
    //         fee!(0.0006),
    //     )
    //     .unwrap();
    //     let config = Config::new(quote!(10000), 100, contract_spec.clone(), 3600).unwrap();
    //     let transaction_accounting = InMemoryTransactionAccounting::from_accounts([
    //         TAccount::from_parts(
    //             quote!(103473101.929093160000000000),
    //             quote!(103473101.201668800000000000),
    //         ),
    //         TAccount::from_parts(quote!(96205857.958312340), quote!(96205054.201693160)),
    //         TAccount::from_parts(quote!(7227008.62800), quote!(7217875.074600000000000000)),
    //         TAccount::from_parts(quote!(2889.542256460), quote!(0)),
    //         TAccount::from_parts(quote!(0), quote!(0)),
    //         TAccount::from_parts(quote!(37345.073100000000000000), quote!(50172.65280)),
    //     ]);

    //     let order_id: OrderId = 16835.into();
    //     let mut active_limit_orders =
    //         HashMap::<OrderId, LimitOrder<BaseCurrency, u64, Pending<BaseCurrency>>>::new();
    //     let meta = ExchangeOrderMeta::new(order_id, 1656633071479000000.into());
    //     let mut status = Pending::new(meta);
    //     status.filled_quantity = FilledQuantity::Filled {
    //         cumulative_qty: base!(0.466),
    //         avg_price: quote!(19599.9),
    //     };
    //     let pending_order =
    //         LimitOrder::from_parts(17503, Side::Buy, quote!(19599.9), base!(0.041), status);
    //     let mut lookup_order_nonce_from_user_order_id = HashMap::<u64, OrderId>::new();
    //     lookup_order_nonce_from_user_order_id
    //         .insert(*pending_order.user_order_id(), pending_order.id());
    //     active_limit_orders.insert(order_id, pending_order);

    //     let mut exchange =
    //         Exchange::<_, BaseCurrency, u64, InMemoryTransactionAccounting<QuoteCurrency>> {
    //             config,
    //             market_state: MarketState::from_components(
    //                 quote!(19599.0),
    //                 quote!(19600.7),
    //                 1656648245348000000.into(),
    //                 963211121,
    //             ),
    //             account_tracker: NoAccountTracker,
    //             risk_engine: IsolatedMarginRiskEngine::new(contract_spec),
    //             next_order_id: 16836.into(),
    //             transaction_accounting,
    //             position: Position::Long(PositionInner::from_parts(base!(0.466), quote!(19599.9))),
    //             active_limit_orders: active_limit_orders.clone(),
    //             lookup_order_nonce_from_user_order_id,
    //             order_margin: OrderMargin::from_parts(quote!(0.160719180), active_limit_orders),
    //             sample_returns_trigger: SampleReturnsTrigger::new(3600_000_000_000.into()),
    //         };
    //     let market_update = Trade {
    //         price: quote!(19598.7),
    //         quantity: base!(1.181),
    //         side: Side::Sell,
    //     };
    //     exchange.check_active_orders(&market_update, 0.into());
    // }

    // #[test]
    // #[tracing_test::traced_test]
    // fn some_more_debugging() {
    //     let contract_spec = ContractSpecification::new(
    //         leverage!(1),
    //         Dec!(0.5),
    //         PriceFilter::new(None, None, quote!(0.1), Dec!(2), Dec!(0.5)).unwrap(),
    //         QuantityFilter::new(None, None, base!(0.001)).unwrap(),
    //         fee!(0.0002),
    //         fee!(0.0006),
    //     )
    //     .unwrap();
    //     let config = Config::new(quote!(10000), 100, contract_spec.clone(), 3600).unwrap();
    //     let transaction_accounting = InMemoryTransactionAccounting::from_accounts([
    //         TAccount::from_parts(
    //             quote!(103473905.685712340000000000),
    //             quote!(103473904.958287980000000000),
    //         ),
    //         TAccount::from_parts(quote!(96205857.958312340), quote!(96205857.958312340)),
    //         TAccount::from_parts(quote!(7227812.22390), quote!(7217875.074600000000000000)),
    //         TAccount::from_parts(quote!(2889.702975640), quote!(0)),
    //         TAccount::from_parts(quote!(0), quote!(0)),
    //         TAccount::from_parts(quote!(37345.073100000000000000), quote!(50172.65280)),
    //     ]);

    //     let active_limit_orders =
    //         HashMap::<OrderId, LimitOrder<BaseCurrency, u64, Pending<BaseCurrency>>>::new();
    //     let lookup_order_nonce_from_user_order_id = HashMap::<u64, OrderId>::new();

    //     let mut exchange =
    //         Exchange::<_, BaseCurrency, u64, InMemoryTransactionAccounting<QuoteCurrency>> {
    //             config,
    //             market_state: MarketState::from_components(
    //                 quote!(19551.6),
    //                 quote!(19551.9),
    //                 1656648418696000000.into(),
    //                 963240098,
    //             ),
    //             account_tracker: NoAccountTracker,
    //             risk_engine: IsolatedMarginRiskEngine::new(contract_spec),
    //             next_order_id: 16836.into(),
    //             transaction_accounting,
    //             position: Position::Long(PositionInner::from_parts(base!(0.507), quote!(19599.9))),
    //             active_limit_orders: active_limit_orders.clone(),
    //             lookup_order_nonce_from_user_order_id,
    //             order_margin: OrderMargin::from_parts(quote!(0), active_limit_orders),
    //             sample_returns_trigger: SampleReturnsTrigger::new(3600_000_000_000.into()),
    //         };
    //     let new_order =
    //         LimitOrder::new_with_user_order_id(Side::Sell, quote!(19869.9), base!(0.507), 17504)
    //             .unwrap();
    //     let meta = ExchangeOrderMeta::new(16836.into(), 1656648418696000000.into());
    //     let pending_order = new_order.into_pending(meta);

    //     exchange.append_limit_order(pending_order);
    // }
}
