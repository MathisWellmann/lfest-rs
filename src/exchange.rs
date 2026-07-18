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
        LimitOrderEvent,
        LimitOrderFill,
        MarginCurrency,
        MarketOrder,
        MaxNumberOfActiveOrders,
        NewOrder,
        OrderId,
        Pending,
        RiskError,
        Side::*,
        Solvency,
        SubmitLimitOrderError,
        SubmitMarketOrderError,
        TimestampNs,
        UserOrderId,
    },
};

/// The resting limit orders which the venue force-cancelled to keep the account's
/// required collateral covered by its equity (margin call).
pub type ForcedCancels<I, const D: u8, BaseOrQuote, UserOrderIdT> =
    Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>;

/// The result of a settled market order: the fill itself together with every side effect
/// of settling it, emitted atomically so the caller cannot miss account-changing events.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MarketOrderSettlement<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The market order in its filled state.
    pub filled_order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
    /// The resting limit orders the venue force-cancelled to keep the account's required
    /// collateral covered after this fill (margin call).
    /// Empty unless the fill reduced or closed the position.
    pub forced_cancels: ForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,
    /// The solvency of the account after settlement and collateral reconciliation.
    pub solvency: Solvency,
}

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

    /// The limit order events (fills and forced cancellations) of the most recent
    /// [`Exchange::update_state`] call, in occurrence order.
    ///
    /// This getter matters after `update_state` returned `Err(RiskError::Liquidate)`:
    /// the error return cannot hand out the events, yet a liquidation force-cancels
    /// every resting order and those cancellations are recorded here.
    // Buffer kept to avoid allocations in hot-paths.
    #[getset(get = "pub")]
    limit_order_events: Vec<LimitOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>,

    /// Scratch buffer collecting force-cancelled orders during a collateral
    /// reconciliation, routed into `limit_order_events` or a `MarketOrderSettlement`.
    forced_cancel_scratch: ForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,

    /// Whether a fill-triggered reconciliation liquidated or bankrupted the account
    /// during the current `update_state` call.
    liquidated_during_fills: bool,

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
        let init_margin_req = config.contract_spec().init_margin_req();
        let maker_fee = *config.contract_spec().fee_maker().as_ref();
        Self {
            config,
            market_state,
            risk_engine,
            next_order_id: OrderId::default(),
            account: Account::new(balances, max_active_orders, init_margin_req, maker_fee),
            // Bids and asks each have a capacity of `max_active_orders`, so one update
            // can emit at most `2 * max_active_orders` fills plus as many forced cancels.
            limit_order_events: Vec::with_capacity(usize::from(max_active_orders.get()) * 4),
            forced_cancel_scratch: Vec::with_capacity(usize::from(max_active_orders.get()) * 2),
            liquidated_during_fills: false,
            order_rate_limiter,
        }
    }

    /// Update the exchange state with new information
    /// Returns a reference to the event vector for performance reasons.
    ///
    /// ### Parameters:
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// If Ok, the limit order events of this update in occurrence order: partial and full
    /// fills as well as resting orders the venue force-cancelled to keep the account's
    /// required collateral covered (margin call).
    /// `Err(RiskError::Liquidate)` means the position was force-closed, either because
    /// the market crossed its liquidation price or because a fill left the equity below
    /// the maintenance margin; the accompanying forced cancellations are then available
    /// through [`Exchange::limit_order_events`].
    pub fn update_state<U>(
        &mut self,
        market_update: &U,
    ) -> Result<&Vec<LimitOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>, RiskError>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        self.limit_order_events.clear();
        self.liquidated_during_fills = false;

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
            self.force_liquidate();
            self.drain_forced_cancels_into_events();
            return Err(e);
        };

        self.check_active_orders(market_update.clone());
        if self.liquidated_during_fills {
            core::hint::cold_path();
            return Err(RiskError::Liquidate);
        }
        Ok(&self.limit_order_events)
    }

    /// Set the best bid and ask, alternatively a `Bba` `MarketUpdate` can be passed into `update_state`
    #[inline]
    pub fn set_best_bid_and_ask(&mut self, bid: QuoteCurrency<I, D>, ask: QuoteCurrency<I, D>) {
        debug_assert!(bid < ask);
        self.market_state.set_bid(bid);
        self.market_state.set_ask(ask);
    }

    /// Force-close the position like a real venue's liquidation engine:
    /// first cancel every resting limit order of the account (buffering them in the
    /// forced-cancel scratch), then close the position with an internal fill at the
    /// current bid or ask.
    ///
    /// This deliberately bypasses the order rate limiter and every admission check,
    /// because a forced liquidation must never fail. A realized loss exceeding the
    /// account equity is absorbed by the venue as `Balances::bad_debt`, so this
    /// method cannot panic on bankrupting fills either.
    fn force_liquidate(&mut self) {
        warn!("liquidating position {}", self.account.position());
        assert2::debug_assert!(self.market_state.ask() > QuoteCurrency::zero());
        assert2::debug_assert!(self.market_state.bid() > QuoteCurrency::zero());
        assert2::debug_assert!(
            !self.account.position().quantity().is_zero(),
            "A neutral position can not be liquidated"
        );

        loop {
            let Some(order_id) = self
                .account
                .active_limit_orders()
                .iter()
                .next()
                .map(|order| order.id())
            else {
                break;
            };
            let cancelled = self
                .account
                .cancel_limit_order(CancelBy::OrderId(order_id))
                .expect("the id belongs to an active order");
            self.forced_cancel_scratch
                .push_within_capacity(cancelled)
                .expect(EXPECT_CAPACITY);
        }

        let position_qty = self.account.position().quantity();
        let (side, fill_price) = if position_qty.is_negative() {
            (Buy, self.market_state.ask())
        } else {
            (Sell, self.market_state.bid())
        };
        let quantity = position_qty.abs();
        let notional = BaseOrQuote::PairedCurrency::convert_from(quantity, fill_price);
        let fee = notional * *self.config.contract_spec().fee_taker().as_ref();
        self.account
            .change_position(quantity, fill_price, side, fee);
        info!("balances after liquidation: {}", self.account.balances());
    }

    /// Reconcile the account collateral after a fill was settled.
    ///
    /// Position-reducing fills are never rejected by the venue, so settling one can leave
    /// the account equity below the canonical requirement (`Account::required_collateral`):
    /// the fill pays fees, may realize a loss and shrinks the position notional which
    /// offset resting reduce-side limit orders. Mirroring a real venue, the exchange then:
    ///
    /// 1. force-closes the position if its maintenance margin is no longer covered by the
    ///    equity (complementing the price-based liquidation check in `update_state`);
    /// 2. force-cancels resting limit orders - largest collateral contributor first, so as
    ///    few orders as possible are cancelled - until the requirement is covered again;
    /// 3. reports the resulting [`Solvency`], where `bad_debt_before` is the reference
    ///    point deciding whether this settlement bankrupted the account.
    ///
    /// The cancelled orders are buffered in the forced-cancel scratch, which the caller
    /// routes into its atomic result (a [`MarketOrderSettlement`] or the event stream).
    #[must_use]
    fn reconcile_margin(&mut self, bad_debt_before: BaseOrQuote::PairedCurrency) -> Solvency {
        let maintenance_margin_req = self.config.contract_spec().maintenance_margin();
        let liquidated = if !self.account.position().quantity().is_zero()
            && self.account.balances().equity()
                < self.account.position().notional() * maintenance_margin_req
        {
            core::hint::cold_path();
            self.force_liquidate();
            true
        } else {
            false
        };

        while self.account.margin_excess() < Zero::zero() {
            core::hint::cold_path();
            let Some(victim_id) = self.account.largest_collateral_contributor() else {
                // No resting orders remain; the equity is below the position's initial
                // margin requirement but still covers its maintenance margin. The account
                // may not increase its risk (the available balance is zero) but keeps the
                // position.
                break;
            };
            let cancelled = self
                .account
                .cancel_limit_order(CancelBy::OrderId(victim_id))
                .expect("the id belongs to an active order which was just looked up");
            warn!(
                "margin call: force-cancelling limit order {} to cover the required collateral",
                cancelled.id()
            );
            self.forced_cancel_scratch
                .push_within_capacity(cancelled)
                .expect(EXPECT_CAPACITY);
        }

        if self.account.balances().bad_debt() > bad_debt_before {
            core::hint::cold_path();
            Solvency::Bankrupt
        } else if liquidated {
            core::hint::cold_path();
            Solvency::Liquidated
        } else if self.account.margin_excess() < Zero::zero() {
            core::hint::cold_path();
            Solvency::InitialMarginDeficit
        } else {
            Solvency::Solvent
        }
    }

    /// Route the forced cancellations of a reconciliation into the event stream of
    /// [`Exchange::update_state`], preserving their order.
    fn drain_forced_cancels_into_events(&mut self) {
        for i in 0..self.forced_cancel_scratch.len() {
            self.limit_order_events
                .push_within_capacity(LimitOrderEvent::ForcedCancel(
                    self.forced_cancel_scratch[i].clone(),
                ))
                .expect(EXPECT_CAPACITY);
        }
        self.forced_cancel_scratch.clear();
    }

    /// Submit a new `MarketOrder` to the exchange.
    ///
    /// A position-reducing order is never rejected for balance reasons; any collateral
    /// shortfall its settlement causes is reconciled by the venue's margin call and
    /// reported in the returned [`MarketOrderSettlement`].
    ///
    /// # Arguments:
    /// `order`: The order that is being submitted.
    ///
    /// # Returns:
    /// If Ok, the settlement of the immediately filled order: the fill itself, the
    /// resting limit orders the venue force-cancelled because of it and the resulting
    /// account [`Solvency`].
    /// Else its an error.
    pub fn submit_market_order(
        &mut self,
        order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<MarketOrderSettlement<I, D, BaseOrQuote, UserOrderIdT>, SubmitMarketOrderError>
    {
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
        let (forced_cancels, solvency) = self.settle_filled_market_order(filled_order.clone());

        Ok(MarketOrderSettlement {
            filled_order,
            forced_cancels,
            solvency,
        })
    }

    /// Settle an immediately filled market order and reconcile the account collateral,
    /// returning the forced cancellations and the resulting solvency.
    fn settle_filled_market_order(
        &mut self,
        order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
    ) -> (ForcedCancels<I, D, BaseOrQuote, UserOrderIdT>, Solvency) {
        let filled_qty = order.quantity();
        assert2::debug_assert!(filled_qty > BaseOrQuote::zero());
        let fill_price = order.state().avg_fill_price();
        assert2::debug_assert!(fill_price > QuoteCurrency::zero());

        let notional = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price);
        let fee = notional * *self.config.contract_spec().fee_taker().as_ref();

        let bad_debt_before = self.account.balances().bad_debt();
        self.account
            .change_position(filled_qty, fill_price, order.side(), fee);

        // A position-reducing fill settles without a prior risk check; the venue
        // reconciles any collateral shortfall instead of rejecting the reduction.
        let solvency = self.reconcile_margin(bad_debt_before);
        // Move the cancelled orders out while retaining the scratch buffer's capacity,
        // which `push_within_capacity` relies on. Allocation-free when empty.
        let mut forced_cancels = ForcedCancels::with_capacity(self.forced_cancel_scratch.len());
        for order in self.forced_cancel_scratch.drain(..) {
            forced_cancels
                .push_within_capacity(order)
                .expect(EXPECT_CAPACITY);
        }
        (forced_cancels, solvency)
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
        // Clear any potential order events from the previous iteration.
        self.limit_order_events.clear();

        if !U::CAN_FILL_LIMIT_ORDERS {
            return;
        }

        if market_update.can_fill_bids() {
            // peek at the best bid order.
            while let Some(order) = self.account.active_limit_orders().best_bid() {
                if let Some((filled_qty, exhausted)) = market_update.limit_order_filled(order) {
                    let bad_debt_before = self.account.balances().bad_debt();
                    let limit_order_update = self.fill_limit_order(
                        order.clone(),
                        filled_qty,
                        market_update.timestamp_exchange_ns(),
                    );
                    self.limit_order_events
                        .push_within_capacity(LimitOrderEvent::Fill(limit_order_update))
                        .expect(EXPECT_CAPACITY);
                    // A fill which reduced the position settles without a prior risk
                    // check; the venue reconciles any collateral shortfall it caused.
                    let solvency = self.reconcile_margin(bad_debt_before);
                    self.drain_forced_cancels_into_events();
                    if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
                        core::hint::cold_path();
                        self.liquidated_during_fills = true;
                        return;
                    }
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
                    let bad_debt_before = self.account.balances().bad_debt();
                    let limit_order_update = self.fill_limit_order(
                        order.clone(),
                        filled_qty,
                        market_update.timestamp_exchange_ns(),
                    );
                    self.limit_order_events
                        .push_within_capacity(LimitOrderEvent::Fill(limit_order_update))
                        .expect(EXPECT_CAPACITY);
                    // A fill which reduced the position settles without a prior risk
                    // check; the venue reconciles any collateral shortfall it caused.
                    let solvency = self.reconcile_margin(bad_debt_before);
                    self.drain_forced_cancels_into_events();
                    if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
                        core::hint::cold_path();
                        self.liquidated_during_fills = true;
                        return;
                    }
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
