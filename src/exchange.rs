use assert2::assert;
use getset::{
    Getters,
    MutGetters,
};
use num_traits::Zero;
use tracing::{
    debug,
    info,
    trace,
    warn,
};

use crate::{
    EXPECT_CAPACITY,
    account::{
        Account,
        Balances,
    },
    config::Config,
    market_state::MarketState,
    order_rate_limiter::OrderRateLimiter,
    prelude::{
        Currency,
        MarketUpdate,
        Mon,
        Position,
        QuoteCurrency,
        RePricing,
    },
    risk_engine::{
        IsolatedMarginRiskEngine,
        RiskEngine,
    },
    types::{
        AmendLimitOrderError,
        CancelBy,
        CancelLimitOrderError,
        ExchangeOrderMeta,
        Filled,
        LimitOrder,
        LimitOrderFill,
        MarginCurrency,
        MarketOrder,
        MaxNumberOfActiveOrders,
        NewOrder,
        OrderId,
        Pending,
        RiskError,
        Side::*,
        SubmitLimitOrderError,
        SubmitMarketOrderError,
        TimestampNs,
        UserOrderId,
    },
};

/// The main leveraged futures exchange for simulated trading
#[derive(Debug, Clone, Getters, MutGetters)]
pub struct Exchange<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The exchange configuration.
    #[getset(get = "pub")]
    config: Config<I, D, BaseOrQuote::PairedCurrency>,

    /// The current state of the simulated market.
    #[getset(get = "pub")]
    market_state: MarketState<I, D>,

    risk_engine: IsolatedMarginRiskEngine<I, D, BaseOrQuote>,

    next_order_id: OrderId,

    /// The account contains the position and balance.
    #[getset(get = "pub")]
    account: Account<I, D, BaseOrQuote, UserOrderIdT>,

    // To avoid allocations in hot-paths
    limit_order_updates: Vec<LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT>>,

    order_rate_limiter: OrderRateLimiter,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Exchange<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as information source
    pub fn new(config: Config<I, D, BaseOrQuote::PairedCurrency>) -> Self {
        let market_state = MarketState::default();
        let risk_engine = IsolatedMarginRiskEngine::new(config.contract_spec().clone());

        let max_active_orders = config.max_num_open_orders();
        let order_rate_limiter =
            OrderRateLimiter::new(config.order_rate_limits().orders_per_second());
        let balances = Balances::new(config.starting_wallet_balance());
        Self {
            config,
            market_state,
            risk_engine,
            next_order_id: OrderId::default(),
            account: Account::new(balances, max_active_orders),
            limit_order_updates: Vec::with_capacity(max_active_orders.get().into()),
            order_rate_limiter,
        }
    }

    /// Update the exchange state with new information
    /// Returns a reference to order updates vector for performance reasons.
    ///
    /// ### Parameters:
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// If Ok, returns updates regarding limit orders, wether partially filled or fully.
    pub fn update_state<U>(
        &mut self,
        market_update: &U,
    ) -> Result<&Vec<LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT>>, RiskError>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        self.market_state
            .update_state(market_update, self.config.contract_spec().price_filter());

        if let Err(e) = <IsolatedMarginRiskEngine<I, D, BaseOrQuote> as RiskEngine<
            I,
            D,
            BaseOrQuote,
            UserOrderIdT,
        >>::check_maintenance_margin(
            &self.risk_engine,
            &self.market_state,
            self.account.position(),
        ) {
            core::hint::cold_path();
            self.liquidate();
            return Err(e);
        };

        self.check_active_orders(market_update.clone());
        Ok(&self.limit_order_updates)
    }

    /// Set the best bid and ask, alternatively a `Bba` `MarketUpdate` can be passed into `update_state`
    #[inline]
    pub fn set_best_bid_and_ask(&mut self, bid: QuoteCurrency<I, D>, ask: QuoteCurrency<I, D>) {
        debug_assert!(bid < ask);
        self.market_state.set_bid(bid);
        self.market_state.set_ask(ask);
    }

    // Liquidate the position by closing it with a market order.
    fn liquidate(&mut self) {
        warn!("liquidating position {}", self.account.position());
        assert2::debug_assert!(self.market_state.ask() > QuoteCurrency::zero());
        assert2::debug_assert!(self.market_state.bid() > QuoteCurrency::zero());
        use Position::*;
        let order = match self.account.position() {
            Long(pos) => MarketOrder::new(Sell, pos.quantity()).expect("Can create market order."),
            Short(pos) => MarketOrder::new(Buy, pos.quantity()).expect("Can create market order."),
            Neutral => panic!("A neutral position can not be liquidated"),
        };
        self.submit_market_order(order)
            .expect("Must be able to submit liquidation order");
        info!("balances after liquidation: {}", self.account.balances());
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
        order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
        SubmitMarketOrderError,
    > {
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
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

        assert2::debug_assert!(self.market_state.ask() > QuoteCurrency::zero());
        assert2::debug_assert!(self.market_state.bid() > QuoteCurrency::zero());
        let fill_price = match order.side() {
            Buy => self.market_state.ask(),
            Sell => self.market_state.bid(),
        };
        self.risk_engine
            .check_market_order(&self.account, &order, fill_price)?;

        let filled_order = order.into_filled(fill_price, self.market_state.current_timestamp_ns());
        self.settle_filled_market_order(filled_order.clone());

        Ok(filled_order)
    }

    fn settle_filled_market_order(
        &mut self,
        order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
    ) {
        let filled_qty = order.quantity();
        assert2::debug_assert!(filled_qty > BaseOrQuote::zero());
        let fill_price = order.state().avg_fill_price();
        assert2::debug_assert!(fill_price > QuoteCurrency::zero());

        let notional = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price);
        let fee = notional * *self.config.contract_spec().fee_taker().as_ref();

        self.account
            .change_position(filled_qty, fill_price, order.side(), fee);
    }

    #[inline(always)]
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
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        SubmitLimitOrderError,
    > {
        trace!("submit_order: {}", order);

        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
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

        self.risk_engine.check_limit_order(&self.account, &order)?;

        // If a limit order is marketable, it will take liquidity from the book at the `limit_price` price level and pay the taker fee,
        let marketable = match order.side() {
            Buy => order.limit_price() >= self.market_state.ask(),
            Sell => order.limit_price() <= self.market_state.bid(),
        };
        match order.re_pricing() {
            RePricing::GoodTilCrossing => {
                if marketable {
                    return Err(SubmitLimitOrderError::GoodTillCrossingRejectedOrder {
                        limit_price: order.limit_price().to_string(),
                        away_market_quotation_price: match order.side() {
                            Buy => self.market_state.ask().to_string(),
                            Sell => self.market_state.bid().to_string(),
                        },
                    });
                }
            }
        }

        self.append_limit_order(order.clone())?;

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
        mut new_order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        AmendLimitOrderError,
    > {
        use AmendLimitOrderError::*;

        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        let existing_order = self
            .account
            .active_limit_orders()
            .get_by_id(existing_order_id, new_order.side()) // Its assumed that `new_order` has the same side as existing order.
            .ok_or_else(|| {
                if existing_order_id < self.next_order_id {
                    OrderNoLongerActive
                } else {
                    OrderIdNotFound {
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
            return Err(AmendQtyAlreadyFilled);
        }

        new_order.set_remaining_quantity(new_leaves_qty);

        self.cancel_limit_order_no_rate_limit(CancelBy::OrderId(existing_order_id))
            .expect("Can always cancel the order here");
        let order = self.submit_limit_order(new_order)?;
        Ok(order)
    }

    /// Append a new limit order as active order.
    /// If limit order is `marketable`, the order will take liquidity from the book at the `limit_price` price level.
    /// Then it pays the taker fee for the quantity that was taken from the book, the rest of the quantity (if any)
    /// will be placed into the book as a passive order.
    fn append_limit_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        self.account.try_insert_order(order)?;
        debug_assert!(if self.account.active_limit_orders().is_empty() {
            self.account.order_margin().is_zero()
        } else {
            true
        });
        self.account.balances().debug_assert_state();

        Ok(())
    }

    /// Cancel an active limit order.
    /// returns Some order if successful with given order_id
    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    pub fn cancel_limit_order(
        &mut self,
        cancel_by: CancelBy<UserOrderIdT>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        CancelLimitOrderError<UserOrderIdT>,
    > {
        trace!("cancel_order: by {:?}", cancel_by);
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        self.cancel_limit_order_no_rate_limit(cancel_by)
    }

    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    fn cancel_limit_order_no_rate_limit(
        &mut self,
        cancel_by: CancelBy<UserOrderIdT>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        CancelLimitOrderError<UserOrderIdT>,
    > {
        let removed_order = self.account.cancel_limit_order(cancel_by)?;

        assert!(if self.account.active_limit_orders().is_empty() {
            self.account.order_margin().is_zero()
        } else {
            true
        });

        Ok(removed_order)
    }

    /// Checks for the execution of active limit orders in the account.
    /// NOTE: only public for benchmarking purposes.
    pub fn check_active_orders<U>(&mut self, mut market_update: U)
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        // Clear any potential order updates from the previous iteration.
        self.limit_order_updates.clear();

        if !U::CAN_FILL_LIMIT_ORDERS {
            return;
        }

        if market_update.can_fill_bids() {
            // peek at the best bid order.
            while let Some(order) = self.account.active_limit_orders().best_bid() {
                if let Some((filled_qty, exhausted)) = market_update.limit_order_filled(order) {
                    let limit_order_update = self.fill_limit_order(
                        order.clone(),
                        filled_qty,
                        market_update.timestamp_exchange_ns(),
                    );
                    self.limit_order_updates
                        .push_within_capacity(limit_order_update)
                        .expect(EXPECT_CAPACITY);
                    if exhausted {
                        return;
                    }
                } else {
                    // We can be sure that no other bid can be filled if this one could not be filled.
                    break;
                }
            }
        }

        if market_update.can_fill_asks() {
            while let Some(order) = self.account.active_limit_orders().best_ask() {
                if let Some((filled_qty, exhausted)) = market_update.limit_order_filled(order) {
                    let limit_order_update = self.fill_limit_order(
                        order.clone(),
                        filled_qty,
                        market_update.timestamp_exchange_ns(),
                    );
                    self.limit_order_updates
                        .push_within_capacity(limit_order_update)
                        .expect(EXPECT_CAPACITY);
                    if exhausted {
                        return;
                    }
                } else {
                    // We can be sure that no other ask can be filled if this one could not be filled.
                    break;
                }
            }
        }

        assert2::debug_assert!(if self.account.active_limit_orders().is_empty() {
            self.account.order_margin().is_zero()
        } else {
            true
        });
        self.account.balances().debug_assert_state();
    }

    fn fill_limit_order(
        &mut self,
        // TODO: refactor this as technically ownership does not make sense here as we should reference the `ActiveLimitOrders` one.
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        filled_quantity: BaseOrQuote,
        ts_ns: TimestampNs,
    ) -> LimitOrderFill<I, D, BaseOrQuote, UserOrderIdT> {
        debug!(
            "filled limit {} order {}: {filled_quantity}/{} @ {}",
            order.side(),
            order.id(),
            order.remaining_quantity(),
            order.limit_price()
        );
        assert2::debug_assert!(
            filled_quantity > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );

        let side = order.side();
        let limit_price = order.limit_price();
        let notional = BaseOrQuote::PairedCurrency::convert_from(filled_quantity, limit_price);
        let fee = notional * *self.config().contract_spec().fee_maker().as_ref();

        match self
            .account
            .fill_best(side, filled_quantity, limit_price, fee, ts_ns)
        {
            Some(order_after_fill) => LimitOrderFill::FullyFilled {
                filled_quantity,
                fee,
                order_after_fill,
            },
            None => LimitOrderFill::PartiallyFilled {
                filled_quantity,
                fee,
                order_after_fill: self
                    .account
                    .active_limit_orders()
                    .get_by_id(order.id(), side)
                    .cloned()
                    .expect("Has this active order"),
            },
        }
    }
}
