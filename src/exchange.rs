use assert2::assert;
use getset::{
    Getters,
    MutGetters,
};
use num_traits::Zero;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};
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
    execution_rules::{
        self,
        BufferCapacities,
        fill_flows,
    },
    ledger::{
        Ledger,
        LedgerEntryKind,
    },
    market_state::MarketState,
    order_rate_limiter::OrderRateLimiter,
    prelude::{
        Currency,
        MarketUpdate,
        Mon,
        QuoteCurrency,
    },
    risk_engine::{
        CrossMarginRiskEngine,
        IsolatedMarginRiskEngine,
        MarginMode,
        RiskEngine,
    },
    stop_orders::{
        PendingStops,
        StopCancelReason,
        StopKind,
        StopOrder,
        StopOrderError,
        StopOrderEvent,
        TriggerPriceType,
    },
    types::{
        AmendLimitOrderError,
        CancelBy,
        CancelLimitOrderError,
        ExchangeOrderMeta,
        Filled,
        FundingRate,
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
        TimeInForce,
        TimestampNs,
        UserOrderId,
    },
    utils::reserve_to_capacity,
};

/// Result of submitting a limit order via [`Exchange::submit_limit_order_with_execution`],
/// distinguishing between a resting passive order and an immediately filled marketable order.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LimitOrderSubmission<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The limit order did not cross the spread and now rests on the book.
    Resting(LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>),
    /// The limit order crossed the spread and executed immediately as a taker.
    ImmediatelyFilled {
        /// The filled limit order (with average execution price = best opposite quote).
        filled_order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
        /// Taker fee charged on the immediate execution.
        fee: BaseOrQuote::PairedCurrency,
        /// Resting orders force-cancelled if settling the fill caused a margin call.
        forced_cancels: ForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,
        /// Account solvency after settlement and reconciliation.
        solvency: Solvency,
    },
}

/// The resting limit orders which the venue force-cancelled to keep the account's
/// required collateral covered by its equity (margin call).
pub type ForcedCancels<I, const D: u8, BaseOrQuote, UserOrderIdT> =
    Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>;

/// The result of a settled market order: the fill itself together with every side effect
/// of settling it, emitted atomically so the caller cannot miss account-changing events.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

/// The result of settling a perpetual funding payment, emitted atomically
/// together with any margin-call side effects so the caller cannot miss
/// account-changing events.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FundingSettlement<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The signed nominal cashflow applied to the account equity in margin currency:
    /// negative when the trader paid funding, positive when the trader received
    /// funding, zero when the position was neutral or the rate was zero.
    ///
    /// When a funding payment bankrupts the account, this still reports the full
    /// nominal cashflow while the venue-absorbed shortfall is recorded in
    /// [`Balances::bad_debt`].
    pub cashflow: BaseOrQuote::PairedCurrency,
    /// The resting limit orders the venue force-cancelled to keep the account's
    /// required collateral covered after paying funding (margin call or liquidation).
    pub forced_cancels: ForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,
    /// The solvency of the account after settling funding and reconciling collateral.
    pub solvency: Solvency,
}

/// The main leveraged futures exchange for simulated trading
#[derive(Debug, Getters, MutGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, UserOrderIdT: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, UserOrderIdT: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
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

    cross_risk_engine: CrossMarginRiskEngine<I, D, BaseOrQuote>,

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
    /// reconciliation, routed into `limit_order_events`, a `MarketOrderSettlement`,
    /// or a `FundingSettlement`.
    forced_cancel_scratch: ForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,

    /// Whether a fill-triggered reconciliation liquidated or bankrupted the account
    /// during the current `update_state` call.
    liquidated_during_fills: bool,

    /// Off-book pending conditional / stop orders (zero cost when empty).
    #[getset(get = "pub")]
    pending_stops: PendingStops<I, D, BaseOrQuote, UserOrderIdT>,

    /// Conditional / stop order events emitted during the most recent call which can
    /// elect or force-cancel stops: [`Exchange::update_state`], or a liquidation triggered
    /// by [`Exchange::submit_market_order`], [`Exchange::submit_limit_order_with_execution`]
    /// or [`Exchange::apply_funding`]. Each of these calls clears the buffer first, so it
    /// never holds more events than there can be pending stops and never drops one.
    #[getset(get = "pub")]
    stop_order_events: Vec<StopOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>,

    /// Scratch buffer for elected stops during `update_state`.
    elected_stops_scratch: Vec<(
        StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        QuoteCurrency<I, D>,
    )>,

    /// Optional cashflow / accounting ledger (disabled by default for zero hot-path overhead).
    ledger: Option<Ledger<I, D, BaseOrQuote::PairedCurrency>>,

    order_rate_limiter: OrderRateLimiter,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Clone for Exchange<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Clone the exchange with the pre-allocated buffers of a freshly constructed one.
    ///
    /// `Vec::clone` keeps only the length, so a derived clone left the event and scratch
    /// buffers without spare capacity and the first fill, forced cancel or stop event
    /// panicked in `push_within_capacity`. `Clone` is cold, so restoring the bounds costs
    /// one allocation per non-empty buffer and keeps every push path free of capacity
    /// checks. `PendingStops` retains its own capacity in its `Clone`.
    fn clone(&self) -> Self {
        let mut cloned = Self {
            config: self.config.clone(),
            market_state: self.market_state.clone(),
            risk_engine: self.risk_engine.clone(),
            cross_risk_engine: self.cross_risk_engine.clone(),
            next_order_id: self.next_order_id,
            account: self.account.clone(),
            limit_order_events: self.limit_order_events.clone(),
            forced_cancel_scratch: self.forced_cancel_scratch.clone(),
            liquidated_during_fills: self.liquidated_during_fills,
            pending_stops: self.pending_stops.clone(),
            stop_order_events: self.stop_order_events.clone(),
            elected_stops_scratch: self.elected_stops_scratch.clone(),
            ledger: self.ledger.clone(),
            order_rate_limiter: self.order_rate_limiter.clone(),
        };
        cloned.reserve_preallocated_capacities();
        cloned
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Exchange<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[cfg(test)]
    pub(crate) fn account_mut(&mut self) -> &mut Account<I, D, BaseOrQuote, UserOrderIdT> {
        &mut self.account
    }

    /// Create a new Exchange with the desired config and whether to use candles
    /// as information source
    pub fn new(config: Config<I, D, BaseOrQuote::PairedCurrency>) -> Self {
        let market_state = MarketState::default();
        let risk_engine = IsolatedMarginRiskEngine::new(config.contract_spec().clone());
        let cross_risk_engine = CrossMarginRiskEngine::new(config.contract_spec().clone());

        let max_active_orders = config.max_num_open_orders();
        let order_rate_limiter =
            OrderRateLimiter::new(config.order_rate_limits().orders_per_second());
        let balances = Balances::new(config.starting_wallet_balance());
        let init_margin_req = config.contract_spec().init_margin_req();
        let maker_fee = *config.contract_spec().fee_maker().as_ref();
        let caps = BufferCapacities::for_order_slots(usize::from(max_active_orders.get()));
        Self {
            config,
            market_state,
            risk_engine,
            cross_risk_engine,
            next_order_id: OrderId::default(),
            account: Account::new(balances, max_active_orders, init_margin_req, maker_fee),
            limit_order_events: Vec::with_capacity(caps.order_events()),
            forced_cancel_scratch: Vec::with_capacity(caps.forced_cancels()),
            liquidated_during_fills: false,
            pending_stops: PendingStops::with_capacity(max_active_orders),
            stop_order_events: Vec::with_capacity(caps.stop_events()),
            elected_stops_scratch: Vec::with_capacity(caps.elected_stops()),
            ledger: None,
            order_rate_limiter,
        }
    }

    /// Give the per-update buffers the capacity of a freshly constructed exchange.
    ///
    /// [`Exchange::new`] allocates them up front, [`Exchange::clone`] restores them after
    /// `Vec::clone` dropped the spare capacity. See [`BufferCapacities`].
    fn reserve_preallocated_capacities(&mut self) {
        let caps =
            BufferCapacities::for_order_slots(usize::from(self.config.max_num_open_orders().get()));
        reserve_to_capacity(&mut self.limit_order_events, caps.order_events());
        reserve_to_capacity(&mut self.forced_cancel_scratch, caps.forced_cancels());
        reserve_to_capacity(&mut self.elected_stops_scratch, caps.elected_stops());
        reserve_to_capacity(&mut self.stop_order_events, caps.stop_events());
    }

    /// Enable the opt-in cashflow [`Ledger`] with initial capacity `capacity`, anchored at
    /// the current wallet equity.
    pub fn enable_ledger(&mut self, capacity: usize) {
        self.ledger = Some(Ledger::with_capacity(
            self.account.balances().equity(),
            capacity,
        ));
    }

    /// Borrow the cashflow [`Ledger`] if enabled.
    #[inline]
    #[must_use]
    pub fn ledger(&self) -> Option<&Ledger<I, D, BaseOrQuote::PairedCurrency>> {
        self.ledger.as_ref()
    }

    /// Journal the nominal `flows` of one settlement which started at `equity_before` /
    /// `bad_debt_before`, followed by the venue's credit for any bad debt it created.
    ///
    /// `Balances` floors the equity at zero on a bankrupting cashflow, so the flows alone
    /// would overshoot the wallet; the separate positive
    /// [`LedgerEntryKind::BadDebtAbsorption`] entry keeps every cashflow nominal while
    /// `starting_balance + net_cashflow == equity` stays exact.
    #[inline]
    fn record_ledger_flows<const N: usize>(
        &mut self,
        order_id: Option<OrderId>,
        flows: [(LedgerEntryKind, BaseOrQuote::PairedCurrency); N],
        equity_before: BaseOrQuote::PairedCurrency,
        bad_debt_before: BaseOrQuote::PairedCurrency,
    ) {
        let Some(ledger) = self.ledger.as_mut() else {
            return;
        };
        let ts = self.market_state.current_timestamp_ns();
        let balances = self.account.balances();
        let new_bad_debt = balances.bad_debt() - bad_debt_before;
        assert!(new_bad_debt >= Zero::zero(), "bad debt never decreases");
        let after_flows = ledger.record_flows(ts, None, order_id, &flows, equity_before);
        let equity_after = ledger.record_flows(
            ts,
            None,
            order_id,
            &[(LedgerEntryKind::BadDebtAbsorption, new_bad_debt)],
            after_flows,
        );
        assert_eq!(
            equity_after,
            balances.equity(),
            "the journaled cashflows must explain the wallet change exactly"
        );
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
    ///
    /// A quote-only update of a flat account is cheap enough to be inlined into the
    /// caller's event loop.
    #[inline]
    pub fn update_state<U>(
        &mut self,
        market_update: &U,
    ) -> Result<&Vec<LimitOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>, RiskError>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        // With all paths in one body, LLVM outlined `update_state` (664 B frame), so the
        // market state, account and config were reloaded from memory on every update:
        // 20 -> 77 retired instructions per update and 2.4x slower on
        // `benches/update_state.rs`. Only the provably event-free case stays here.
        //
        // A flat account cannot be liquidated and has nothing to force-cancel, and an
        // update which cannot fill leaves resting orders alone, so this update only
        // refreshes the market state and clears the event buffers.
        let quote_only = !U::CAN_FILL_LIMIT_ORDERS
            && self.pending_stops.is_empty()
            && matches!(self.config.margin_mode(), MarginMode::Isolated)
            && self.account.position().quantity().is_zero();

        if !quote_only {
            return self.update_state_checked(market_update);
        }

        self.begin_update(market_update);
        Ok(&self.limit_order_events)
    }

    /// The full update path: pending stops, limit order fills, cross margin and
    /// liquidation.
    ///
    /// Outlined on purpose. It is the same work as the quote-only hot path in
    /// [`Self::update_state`] except that it can emit events, so inlining it there would
    /// push the caller over LLVM's inlining budget and cost far more than this call.
    #[inline(never)]
    fn update_state_checked<U>(
        &mut self,
        market_update: &U,
    ) -> Result<&Vec<LimitOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>, RiskError>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        self.begin_update(market_update);

        if !self.pending_stops.is_empty() {
            self.evaluate_pending_stops(market_update.price_range());
            if self.liquidated_during_fills {
                core::hint::cold_path();
                return Err(RiskError::Liquidate);
            }
        }

        let mm_check = match self.config.margin_mode() {
            MarginMode::Isolated => self.check_isolated_maintenance_margin(),
            MarginMode::Cross => self
                .cross_risk_engine
                .check_mark_to_market_maintenance_margin(&self.market_state, &self.account),
        };
        if let Err(e) = mm_check {
            return self.liquidate_on_margin_call(e);
        }

        self.fill_active_orders(market_update.clone());
        if self.liquidated_during_fills {
            core::hint::cold_path();
            return Err(RiskError::Liquidate);
        }
        Ok(&self.limit_order_events)
    }

    /// Clear the event buffers of the previous update and apply the new market state.
    #[inline]
    fn begin_update<U>(&mut self, market_update: &U)
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        self.limit_order_events.clear();
        self.reset_stop_order_events();
        self.liquidated_during_fills = false;

        self.market_state
            .update_state(market_update, self.config.contract_spec().price_filter());
    }

    /// The isolated maintenance-margin check of the current position.
    #[inline]
    fn check_isolated_maintenance_margin(&self) -> Result<(), RiskError> {
        <IsolatedMarginRiskEngine<I, D, BaseOrQuote> as RiskEngine<
            I,
            D,
            BaseOrQuote,
            UserOrderIdT,
        >>::check_maintenance_margin(
            &self.risk_engine,
            &self.market_state,
            self.account.position(),
        )
    }

    /// Force-close the position on a failed margin check, record the cancellations it
    /// caused, and propagate `error`.
    #[cold]
    #[inline(never)]
    fn liquidate_on_margin_call(
        &mut self,
        error: RiskError,
    ) -> Result<&Vec<LimitOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>, RiskError> {
        self.force_liquidate();
        self.drain_forced_cancels_into_events();
        Err(error)
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
    ///
    /// The taker fee is charged in full, the liquidation clearance fee only up to the
    /// equity left after the realized PnL and the taker fee (see
    /// `execution_rules::capped_clearance_fee`).
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

        if !self.pending_stops.is_empty() {
            let mut drained = Vec::with_capacity(self.pending_stops.len());
            self.pending_stops.drain_all(&mut drained);
            for stop in drained {
                self.push_stop_event(StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::ForcedCancel,
                });
            }
        }

        let position_qty = self.account.position().quantity();
        let (side, fill_price) = if position_qty.is_negative() {
            (Buy, self.market_state.ask())
        } else {
            (Sell, self.market_state.bid())
        };
        let quantity = position_qty.abs();
        let notional = BaseOrQuote::PairedCurrency::convert_from(quantity, fill_price);
        let taker_fee = notional * *self.config.contract_spec().fee_taker().as_ref();
        let eq_before = self.account.balances().equity();
        let bd_before = self.account.balances().bad_debt();
        let realized = self
            .account
            .change_position_realizing(quantity, fill_price, side, taker_fee);
        assert!(self.account.position().quantity().is_zero());
        // Charged after the close, so the cap sees the exact equity left.
        let bd_after_close = self.account.balances().bad_debt();
        let clearance_fee = execution_rules::capped_clearance_fee(
            notional * self.config.contract_spec().liquidation_fee(),
            self.account.balances().equity(),
        );
        self.account.account_for_fee(clearance_fee);
        // The clearance fee alone never creates bad debt.
        assert!(self.account.balances().bad_debt() == bd_after_close);
        assert!(self.account.balances().equity() >= Zero::zero());
        self.record_ledger_flows(
            None,
            [
                (LedgerEntryKind::Liquidation, realized),
                (LedgerEntryKind::TradingFee, -taker_fee),
                (LedgerEntryKind::Liquidation, -clearance_fee),
            ],
            eq_before,
            bd_before,
        );
        info!("balances after liquidation: {}", self.account.balances());
    }

    /// Reconcile the account collateral after a fill or funding settlement.
    ///
    /// Position-reducing fills and periodic funding payments are never rejected by the venue,
    /// so settling one can leave the account equity below the canonical requirement
    /// (`Account::required_collateral`): a fill pays fees, may realize a loss and shrinks the
    /// position notional which offset resting reduce-side limit orders; paying funding directly
    /// debits equity. Mirroring a real venue, the exchange then:
    ///
    /// 1. force-closes the position if its maintenance margin is no longer covered by the
    ///    equity (complementing the price-based liquidation check in `update_state`);
    /// 2. force-cancels resting limit orders - largest collateral contributor first, so as
    ///    few orders as possible are cancelled - until the requirement is covered again;
    /// 3. reports the resulting [`Solvency`], where `bad_debt_before` is the reference
    ///    point deciding whether this settlement bankrupted the account.
    ///
    /// The cancelled orders are buffered in the forced-cancel scratch, which the caller
    /// routes into its atomic result (a [`MarketOrderSettlement`], a [`FundingSettlement`],
    /// or the event stream).
    #[must_use]
    fn reconcile_margin(&mut self, bad_debt_before: BaseOrQuote::PairedCurrency) -> Solvency {
        let should_liquidate = match self.config.margin_mode() {
            MarginMode::Isolated => {
                !self.account.position().quantity().is_zero()
                    && self.account.balances().equity()
                        < self
                            .config
                            .contract_spec()
                            .maintenance_margin_schedule()
                            .maintenance_margin(self.account.position().notional())
            }
            MarginMode::Cross => {
                if self.account.position().quantity().is_zero() {
                    false
                } else {
                    let has_positive_quotes = self.market_state.bid() > QuoteCurrency::zero()
                        && self.market_state.ask() > QuoteCurrency::zero();
                    if has_positive_quotes {
                        self.cross_risk_engine
                            .check_mark_to_market_maintenance_margin(
                                &self.market_state,
                                &self.account,
                            )
                            .is_err()
                    } else {
                        let mm = self
                            .config
                            .contract_spec()
                            .maintenance_margin_schedule()
                            .maintenance_margin(self.account.position().notional());
                        self.account.balances().equity() < mm
                    }
                }
            }
        };
        let liquidated = if should_liquidate {
            core::hint::cold_path();
            self.force_liquidate();
            true
        } else {
            self.prune_stale_reduce_only_orders();
            false
        };

        let effective_excess =
            |account: &Account<I, D, BaseOrQuote, UserOrderIdT>,
             market_state: &MarketState<I, D>,
             config: &Config<I, D, BaseOrQuote::PairedCurrency>| {
                match config.margin_mode() {
                    MarginMode::Isolated => account.margin_excess(),
                    MarginMode::Cross => {
                        let has_positive_quotes = market_state.bid() > QuoteCurrency::zero()
                            && market_state.ask() > QuoteCurrency::zero();
                        let upnl = if has_positive_quotes {
                            account.position().unrealized_pnl_marked(
                                market_state,
                                config.contract_spec().mark_method(),
                            )
                        } else {
                            Zero::zero()
                        };
                        account.margin_excess() + upnl
                    }
                }
            };

        while effective_excess(&self.account, &self.market_state, &self.config) < Zero::zero() {
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
        } else if effective_excess(&self.account, &self.market_state, &self.config) < Zero::zero() {
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

    /// Move the force-cancelled orders out of the scratch buffer while retaining its
    /// capacity, which `push_within_capacity` relies on. Allocation-free when empty.
    fn take_forced_cancels(&mut self) -> ForcedCancels<I, D, BaseOrQuote, UserOrderIdT> {
        let mut forced_cancels = ForcedCancels::with_capacity(self.forced_cancel_scratch.len());
        for order in self.forced_cancel_scratch.drain(..) {
            forced_cancels
                .push_within_capacity(order)
                .expect(EXPECT_CAPACITY);
        }
        forced_cancels
    }

    /// Settle a periodic perpetual futures funding payment on the current open position
    /// and reconcile the account collateral.
    ///
    /// Callers pass the venue's funding mark price when available, or
    /// `self.market_state().mid_price()` when historical mark prices are unavailable.
    ///
    /// Sign convention:
    /// - Positive `rate`: longs pay shorts (`cashflow < 0` for longs, `cashflow > 0` for shorts).
    /// - Negative `rate`: shorts pay longs (`cashflow > 0` for longs, `cashflow < 0` for shorts).
    /// - Neutral position or zero `rate`: zero cashflow; collateral is still reconciled so the
    ///   returned [`Solvency`] reflects the account's current state.
    ///
    /// Paying funding can trigger margin-call forced cancellations of resting limit orders,
    /// force-liquidation if equity drops below maintenance margin, or bankruptcy if equity is
    /// exhausted (with the deficit absorbed into `Balances::bad_debt`).
    ///
    /// # Market-state and margin-model notes
    /// - Keep `market_state` up to date before calling `apply_funding`: if funding triggers
    ///   a force-liquidation, the position is closed against the current `market_state`
    ///   best bid/ask rather than `mark_price` (and `bid`/`ask` must be strictly positive).
    /// - Maintenance-margin reconciliation compares wallet equity against entry-price
    ///   position notional (`Position::notional() * maintenance_margin`), without
    ///   unrealized PnL at `mark_price`. Likewise, [`Exchange::update_state`] checks
    ///   price-based liquidation from a static entry-price threshold independent of
    ///   cumulative funding erosion.
    ///
    /// # Panics
    /// Panics if `mark_price <= 0`, or (in debug builds) if funding triggers liquidation
    /// while `market_state` bid/ask have not been initialized to positive prices.
    #[must_use]
    pub fn apply_funding(
        &mut self,
        rate: FundingRate<I, D>,
        mark_price: QuoteCurrency<I, D>,
    ) -> FundingSettlement<I, D, BaseOrQuote, UserOrderIdT> {
        // A liquidation reports its stop cancellations in `stop_order_events`.
        self.reset_stop_order_events();
        let cashflow = self.account.funding_cashflow(rate, mark_price);
        let eq_before = self.account.balances().equity();
        let bad_debt_before = self.account.balances().bad_debt();
        if !cashflow.is_zero() {
            self.account.apply_funding(cashflow);
            self.record_ledger_flows(
                None,
                [(LedgerEntryKind::Funding, cashflow)],
                eq_before,
                bad_debt_before,
            );
        }
        let solvency = self.reconcile_margin(bad_debt_before);
        let forced_cancels = self.take_forced_cancels();

        FundingSettlement {
            cashflow,
            forced_cancels,
            solvency,
        }
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
        mut order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<MarketOrderSettlement<I, D, BaseOrQuote, UserOrderIdT>, SubmitMarketOrderError>
    {
        // A liquidation reports its stop cancellations in `stop_order_events`.
        self.reset_stop_order_events();
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        // Basic checks
        self.config
            .contract_spec()
            .quantity_filter()
            .validate_order_quantity(order.quantity())?;

        if order.reduce_only() {
            let Some(clamped) = self.clamp_reduce_only_quantity(order.side(), order.quantity())
            else {
                return Err(SubmitMarketOrderError::ReduceOnlyRejected);
            };
            if clamped < order.quantity() {
                order.set_quantity(clamped);
            }
        }

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);

        assert2::debug_assert!(self.market_state.ask() > QuoteCurrency::zero());
        assert2::debug_assert!(self.market_state.bid() > QuoteCurrency::zero());
        let impact_quote = self.config.contract_spec().slippage_model().quote(
            &self.market_state,
            order.side(),
            order.quantity(),
            self.config.contract_spec().price_filter().tick_size(),
        );
        let fill_price = impact_quote.execution_price();
        execution_rules::validate_order_notional(
            self.config.contract_spec().notional_filter(),
            order.quantity(),
            fill_price,
            order.reduce_only(),
        )?;
        match self.config.margin_mode() {
            MarginMode::Isolated => {
                self.risk_engine
                    .check_market_order(&self.account, &order, fill_price)?
            }
            MarginMode::Cross => self.cross_risk_engine.check_market_order_with_market(
                &self.account,
                &self.market_state,
                &order,
                fill_price,
            )?,
        }

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

        let eq_before = self.account.balances().equity();
        let bad_debt_before = self.account.balances().bad_debt();
        let realized =
            self.account
                .change_position_realizing(filled_qty, fill_price, order.side(), fee);
        self.record_ledger_flows(
            Some(order.state().meta().id()),
            fill_flows(realized, fee),
            eq_before,
            bad_debt_before,
        );

        // A position-reducing fill settles without a prior risk check; the venue
        // reconciles any collateral shortfall instead of rejecting the reduction.
        let solvency = self.reconcile_margin(bad_debt_before);
        let forced_cancels = self.take_forced_cancels();
        (forced_cancels, solvency)
    }

    /// Return the current mark price of the market using the contract's configured
    /// [`crate::prelude::MarkMethod`].
    #[inline]
    #[must_use]
    pub fn mark_price(&self) -> QuoteCurrency<I, D> {
        self.market_state
            .mark_price(self.config.contract_spec().mark_method())
    }

    /// Return the position's current unrealized PnL evaluated under the contract's
    /// [`crate::prelude::MarkMethod`].
    #[inline]
    #[must_use]
    pub fn unrealized_pnl(&self) -> BaseOrQuote::PairedCurrency {
        self.account.position().unrealized_pnl_marked(
            &self.market_state,
            self.config.contract_spec().mark_method(),
        )
    }

    /// Return live mark-to-market account equity (`wallet_equity + unrealized_pnl`).
    #[inline]
    #[must_use]
    pub fn mark_to_market_equity(&self) -> BaseOrQuote::PairedCurrency {
        self.account.balances().equity() + self.unrealized_pnl()
    }

    /// Estimate the liquidation price of the current position.
    ///
    /// - Under [`MarginMode::Isolated`], returns the static entry-price liquidation
    ///   threshold derived from the tiered maintenance margin schedule at the entry notional.
    /// - Under [`MarginMode::Cross`], returns the dynamic liquidation price accounting
    ///   for actual wallet equity and the active [`crate::prelude::MaintenanceMarginSchedule`]
    ///   bracket (`None` if the position is fully collateralized or neutral).
    #[must_use]
    pub fn estimated_liquidation_price(&self) -> Option<QuoteCurrency<I, D>> {
        let pos = self.account.position();
        if pos.quantity().is_zero() {
            return None;
        }
        match self.config.margin_mode() {
            MarginMode::Isolated => {
                let liq = crate::risk_engine::isolated_liquidation_price(
                    self.config.contract_spec(),
                    pos,
                );
                if liq > QuoteCurrency::zero() {
                    Some(liq)
                } else {
                    None
                }
            }
            MarginMode::Cross => self.cross_risk_engine.estimated_liquidation_price(
                self.account.balances().equity(),
                pos,
                self.mark_price(),
            ),
        }
    }

    /// Return `true` when an order of `(side, quantity)` strictly reduces or closes
    /// the current open position without flipping its direction.
    #[inline]
    fn is_strictly_reducing(&self, side: crate::types::Side, quantity: BaseOrQuote) -> bool {
        let pos_qty = self.account.position().quantity();
        match side {
            Buy => pos_qty.is_negative() && quantity <= pos_qty.abs(),
            Sell => pos_qty.is_positive() && quantity <= pos_qty,
        }
    }

    /// Compute the clamped executable quantity for a `reduce_only` order, or `None`
    /// if the account has no opposite-side position to reduce.
    #[inline]
    fn clamp_reduce_only_quantity(
        &self,
        side: crate::types::Side,
        quantity: BaseOrQuote,
    ) -> Option<BaseOrQuote> {
        execution_rules::clamp_reduce_only_quantity(
            self.account.position().quantity(),
            side,
            quantity,
        )
    }

    /// Clamp a `reduce_only` order which is about to rest to the room the already resting
    /// reduce-only orders of its side leave within the position, because together they
    /// may never exceed it.
    ///
    /// # Errors
    /// [`SubmitLimitOrderError::ReduceOnlyRejected`] if there is no room left.
    fn clamp_resting_reduce_only(
        &self,
        order: &mut LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<(), SubmitLimitOrderError> {
        assert!(order.reduce_only());
        assert!(
            self.is_strictly_reducing(order.side(), order.remaining_quantity()),
            "the reduce-only quantity was clamped to the position before"
        );
        let resting = self
            .account
            .active_limit_orders()
            .reduce_only_remaining(order.side());
        let room = self.account.position().quantity().abs() - resting;
        if room <= BaseOrQuote::zero() {
            return Err(SubmitLimitOrderError::ReduceOnlyRejected);
        }
        if room < order.remaining_quantity() {
            order.set_remaining_quantity(room);
        }
        assert2::debug_assert!(order.remaining_quantity() <= room);
        Ok(())
    }

    /// Force-cancel resting `reduce_only` limit orders which no longer fit within the
    /// open position: all of them once the position closed or flipped, and on the
    /// reducing side those (furthest from the touch first) whose cumulative remaining
    /// quantity exceeds `|position|`. Without this, a reduce-only order admitted against
    /// a larger position could later fill and increase or flip the position.
    fn prune_stale_reduce_only_orders(&mut self) {
        let scratch = &mut self.forced_cancel_scratch;
        self.account.cancel_excess_reduce_only_orders(|cancelled| {
            scratch
                .push_within_capacity(cancelled)
                .expect(EXPECT_CAPACITY);
        });
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> OrderId {
        let oid = self.next_order_id;
        self.next_order_id.incr();
        oid
    }

    /// Submit a limit order with full support for [`TimeInForce`] (`GTX`, `GTC`, `IOC`, `FOK`),
    /// [`crate::prelude::RePricing::Marketable`], and `reduce_only`.
    ///
    /// - If the order crosses the spread and uses `GoodTilCancelled`, `ImmediateOrCancel`,
    ///   `FillOrKill`, or [`crate::prelude::RePricing::Marketable`], it executes immediately at the best
    ///   opposite quote plus the contract's [`crate::prelude::SlippageModel`] impact
    ///   (never beyond `limit_price`), paying the taker fee, and returns
    ///   [`LimitOrderSubmission::ImmediatelyFilled`].
    /// - A `reduce_only` `FillOrKill` order larger than the open position is rejected with
    ///   [`SubmitLimitOrderError::FillOrKillNotFullyFillable`] instead of filling partially.
    /// - If the order does not cross the spread:
    ///   - `ImmediateOrCancel` / `FillOrKill` return
    ///     [`SubmitLimitOrderError::ImmediateExecutionNotMarketable`] without resting.
    ///   - `GoodTilCrossing` / `GoodTilCancelled` rest on the book and return
    ///     [`LimitOrderSubmission::Resting`].
    ///
    /// A liquidation caused by an immediate fill reports the cancelled stop orders in
    /// [`Exchange::stop_order_events`], which this call clears first.
    ///
    /// # Errors
    /// Returns [`SubmitLimitOrderError`] if rate-limited, filter validation fails,
    /// `reduce_only` has no opposite position, margin is insufficient, or TIF rejects.
    pub fn submit_limit_order_with_execution(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<LimitOrderSubmission<I, D, BaseOrQuote, UserOrderIdT>, SubmitLimitOrderError> {
        trace!("submit_limit_order_with_execution: {}", order);
        self.reset_stop_order_events();
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        self.submit_limit_order_no_rate_limit(order)
    }

    /// [`Self::submit_limit_order_with_execution`] without the order rate limiter and
    /// without clearing `stop_order_events`, for orders the venue itself places inside
    /// [`Self::update_state`] (elected stop-limit children): the user's rate limit governs
    /// the user's requests, and rejecting a triggered stop for it would silently drop the
    /// protection the user already paid a rate-limit slot for when submitting the stop.
    fn submit_limit_order_no_rate_limit(
        &mut self,
        mut order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<LimitOrderSubmission<I, D, BaseOrQuote, UserOrderIdT>, SubmitLimitOrderError> {
        self.config
            .contract_spec()
            .quantity_filter()
            .validate_order_quantity(order.remaining_quantity())?;
        let mark_price = self
            .market_state
            .mark_price(self.config.contract_spec().mark_method());
        self.config
            .contract_spec()
            .price_filter()
            .validate_limit_price(order.limit_price(), mark_price)?;

        if order.reduce_only() {
            execution_rules::clamp_reduce_only_limit_order(
                &mut order,
                self.account.position().quantity(),
            )?;
        }

        let marketable = order.is_marketable(&self.market_state);
        let is_post_only = order.is_post_only();
        if !marketable || is_post_only {
            // An executing order is validated at its fill price instead, see
            // `execute_marketable_limit_order`.
            execution_rules::validate_order_notional(
                self.config.contract_spec().notional_filter(),
                order.remaining_quantity(),
                order.limit_price(),
                order.reduce_only(),
            )?;
        }

        if marketable && is_post_only {
            let meta = ExchangeOrderMeta::new(
                self.next_order_id(),
                self.market_state.current_timestamp_ns(),
            );
            let pending = order.into_pending(meta);
            match self.config.margin_mode() {
                MarginMode::Isolated => self
                    .risk_engine
                    .check_limit_order(&self.account, &pending)?,
                MarginMode::Cross => self.cross_risk_engine.check_limit_order_with_market(
                    &self.account,
                    &self.market_state,
                    &pending,
                )?,
            }
            return Err(SubmitLimitOrderError::GoodTillCrossingRejectedOrder {
                limit_price: pending.limit_price().to_string(),
                away_market_quotation_price: match pending.side() {
                    Buy => self.market_state.ask().to_string(),
                    Sell => self.market_state.bid().to_string(),
                },
            });
        }

        if marketable {
            return self.execute_marketable_limit_order(order);
        }

        if matches!(
            order.time_in_force(),
            TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill
        ) {
            return Err(SubmitLimitOrderError::ImmediateExecutionNotMarketable);
        }

        if order.reduce_only() {
            self.clamp_resting_reduce_only(&mut order)?;
        }

        let meta = ExchangeOrderMeta::new(
            self.next_order_id(),
            self.market_state.current_timestamp_ns(),
        );
        let order = order.into_pending(meta);
        match self.config.margin_mode() {
            MarginMode::Isolated => self.risk_engine.check_limit_order(&self.account, &order)?,
            MarginMode::Cross => self.cross_risk_engine.check_limit_order_with_market(
                &self.account,
                &self.market_state,
                &order,
            )?,
        }
        self.append_limit_order(order.clone())?;
        Ok(LimitOrderSubmission::Resting(order))
    }

    fn execute_marketable_limit_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<LimitOrderSubmission<I, D, BaseOrQuote, UserOrderIdT>, SubmitLimitOrderError> {
        assert!(order.is_marketable(&self.market_state));
        let fill_price = execution_rules::marketable_limit_fill_price(
            self.config.contract_spec().slippage_model(),
            self.config.contract_spec().price_filter().tick_size(),
            &self.market_state,
            order.side(),
            order.remaining_quantity(),
            order.limit_price(),
        );
        execution_rules::validate_order_notional(
            self.config.contract_spec().notional_filter(),
            order.remaining_quantity(),
            fill_price,
            order.reduce_only(),
        )?;
        let ts_ns = self.market_state.current_timestamp_ns();
        let meta = ExchangeOrderMeta::new(self.next_order_id(), ts_ns);
        let mkt_probe = MarketOrder::new_with_user_order_id(
            order.side(),
            order.remaining_quantity(),
            order.user_order_id(),
        )
        .expect("limit order remaining quantity is strictly positive")
        .into_pending(meta.clone());
        match self.config.margin_mode() {
            MarginMode::Isolated => {
                self.risk_engine
                    .check_market_order(&self.account, &mkt_probe, fill_price)?
            }
            MarginMode::Cross => self.cross_risk_engine.check_market_order_with_market(
                &self.account,
                &self.market_state,
                &mkt_probe,
                fill_price,
            )?,
        }
        let pending = order.into_pending(meta);
        let filled_qty = pending.remaining_quantity();
        let notional = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price);
        let fee = notional * *self.config.contract_spec().fee_taker().as_ref();
        let eq_before = self.account.balances().equity();
        let bad_debt_before = self.account.balances().bad_debt();
        let realized =
            self.account
                .change_position_realizing(filled_qty, fill_price, pending.side(), fee);
        self.record_ledger_flows(
            Some(pending.id()),
            fill_flows(realized, fee),
            eq_before,
            bad_debt_before,
        );
        let solvency = self.reconcile_margin(bad_debt_before);
        let forced_cancels = self.take_forced_cancels();
        let filled_order = pending.into_filled_at_price(fill_price, ts_ns);
        Ok(LimitOrderSubmission::ImmediatelyFilled {
            filled_order,
            fee,
            forced_cancels,
            solvency,
        })
    }

    /// Submit a resting passive limit order.
    ///
    /// This API never trades: an order which would execute immediately (it crosses the
    /// spread and is not post-only) is rejected with
    /// [`SubmitLimitOrderError::WouldExecuteImmediately`] without touching the account,
    /// because the returned pending order could not report the fill, its taker fee, the
    /// resulting forced cancels or the solvency. Use
    /// [`Self::submit_limit_order_with_execution`] for marketable limit orders
    /// (`RePricing::Marketable`, crossing `GoodTilCancelled`, `ImmediateOrCancel`,
    /// `FillOrKill`). Crossing post-only orders keep their
    /// [`SubmitLimitOrderError::GoodTillCrossingRejectedOrder`] rejection.
    ///
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
        if order.is_marketable(&self.market_state) && !order.is_post_only() {
            return Err(SubmitLimitOrderError::WouldExecuteImmediately);
        }
        // A resting order never settles a fill, so it cannot liquidate or emit stop events.
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        match self.submit_limit_order_no_rate_limit(order)? {
            LimitOrderSubmission::Resting(pending) => {
                assert!(pending.remaining_quantity() > BaseOrQuote::zero());
                Ok(pending)
            }
            LimitOrderSubmission::ImmediatelyFilled { .. } => {
                unreachable!("marketable orders are rejected before submission")
            }
        }
    }

    /// Amend an existing limit order.
    ///
    /// The amend message will only be accepted if the original order can be successfully removed.
    /// Requests which cannot be processed will be rejected with an error; the original
    /// order then stays on the book unchanged (same id and queue priority). The only
    /// exception is [`AmendLimitOrderError::AmendQtyAlreadyFilled`], which cancels it.
    ///
    /// Like [`Self::submit_limit_order`], an amend never trades: a new limit price
    /// crossing the spread is rejected with [`SubmitLimitOrderError::WouldExecuteImmediately`].
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
        // Validate what does not depend on the original order before cancelling it.
        if new_order.is_marketable(&self.market_state) && !new_order.is_post_only() {
            return Err(SubmitLimitOrderError::WouldExecuteImmediately.into());
        }

        new_order.set_remaining_quantity(new_leaves_qty);

        let original = self
            .cancel_limit_order_no_rate_limit(CancelBy::OrderId(existing_order_id))
            .expect("Can always cancel the order here");
        // The admission checks (margin, reduce-only room) must see the book without the
        // original order, so it is removed first and restored if the replacement fails.
        let side = new_order.side();
        match self.submit_limit_order(new_order) {
            Ok(order) => Ok(order),
            Err(e) => {
                self.account
                    .try_insert_order(original)
                    .expect("the slot of the original order was just freed");
                assert!(
                    self.account
                        .active_limit_orders()
                        .get_by_id(existing_order_id, side)
                        .is_some()
                );
                Err(e.into())
            }
        }
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

    /// Cancel every active limit order on the exchange in deterministic order,
    /// charging a single rate-limit slot.
    ///
    /// # Errors
    /// Returns [`crate::order_rate_limiter::RateLimitReached`] if the order rate limit
    /// has been exceeded.
    pub fn cancel_all_limit_orders(
        &mut self,
    ) -> Result<
        crate::types::MassCancelResult<I, D, BaseOrQuote, UserOrderIdT>,
        crate::order_rate_limiter::RateLimitReached,
    > {
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        let cancelled = self.account.cancel_all_limit_orders(None);
        assert!(self.account.active_limit_orders().is_empty());
        assert!(self.account.order_margin().is_zero());
        Ok(crate::types::MassCancelResult::new(cancelled))
    }

    /// Cancel every active limit order on `side` in deterministic order,
    /// charging a single rate-limit slot.
    ///
    /// # Errors
    /// Returns [`crate::order_rate_limiter::RateLimitReached`] if the order rate limit
    /// has been exceeded.
    pub fn cancel_limit_orders_by_side(
        &mut self,
        side: crate::types::Side,
    ) -> Result<
        crate::types::MassCancelResult<I, D, BaseOrQuote, UserOrderIdT>,
        crate::order_rate_limiter::RateLimitReached,
    > {
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        let cancelled = self.account.cancel_all_limit_orders(Some(side));
        if self.account.active_limit_orders().is_empty() {
            assert!(self.account.order_margin().is_zero());
        }
        Ok(crate::types::MassCancelResult::new(cancelled))
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
    ///
    /// The resulting events are appended to [`Exchange::limit_order_events`] without
    /// clearing it first: `update_state` clears the buffer once per update, so events
    /// emitted earlier in the same update (e.g. forced cancels caused by elected stops)
    /// survive. Direct callers must clear the buffer themselves if they need that.
    /// [`Exchange::stop_order_events`] is cleared, as a fill-triggered liquidation reports
    /// its stop cancellations there.
    /// NOTE: only public for benchmarking purposes.
    pub fn check_active_orders<U>(&mut self, market_update: U)
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        self.reset_stop_order_events();
        self.fill_active_orders(market_update);
    }

    /// [`Self::check_active_orders`] without clearing `stop_order_events`, so
    /// `update_state` keeps the stop events emitted earlier in the same update.
    fn fill_active_orders<U>(&mut self, mut market_update: U)
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
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
                        // Only this side's taker flow is used up; a two-sided update
                        // (e.g. `SmartCandle`) may still fill the other side.
                        break;
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
                        // Only this side's taker flow is used up; a two-sided update
                        // (e.g. `SmartCandle`) may still fill the other side.
                        break;
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
        // Defence in depth: `prune_stale_reduce_only_orders` keeps resting reduce-only
        // orders within the position, but a fill must never increase or flip it anyway.
        let filled_quantity = if order.reduce_only() {
            self.clamp_reduce_only_quantity(side, filled_quantity)
                .expect("resting reduce-only orders are pruned after every position change")
        } else {
            filled_quantity
        };
        assert2::debug_assert!(filled_quantity > BaseOrQuote::zero());
        let limit_price = order.limit_price();
        let notional = BaseOrQuote::PairedCurrency::convert_from(filled_quantity, limit_price);
        let fee = notional * *self.config().contract_spec().fee_maker().as_ref();
        let eq_before = self.account.balances().equity();
        let bd_before = self.account.balances().bad_debt();

        let (realized, filled) =
            self.account
                .fill_best_realizing(side, filled_quantity, limit_price, fee, ts_ns);
        let res = match filled {
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
        };
        self.record_ledger_flows(
            Some(order.id()),
            fill_flows(realized, fee),
            eq_before,
            bd_before,
        );
        res
    }

    /// Submit a conditional / stop order (`StopMarket`, `StopLimit`, `TakeProfitMarket`,
    /// `TakeProfitLimit`, or `TrailingStop`) to the off-book pending-stops store.
    ///
    /// Rejects immediately with [`StopOrderError::StopWouldTrigger`] if the current market
    /// reference price already crosses `order.stop_price()` (matching `OrderBook-rs`).
    ///
    /// # Errors
    /// Returns [`StopOrderError`] on rate limit, quantity filter violation, immediate
    /// trigger, or capacity exhaustion.
    pub fn submit_stop_order(
        &mut self,
        mut order: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
    ) -> Result<StopOrder<I, D, BaseOrQuote, UserOrderIdT>, StopOrderError> {
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        self.config
            .contract_spec()
            .quantity_filter()
            .validate_order_quantity(order.quantity())?;

        if let Some(ref_price) = self.stop_reference_price(order.trigger_price_type(), order.side())
            && order.is_triggered_by(ref_price)
        {
            return Err(StopOrderError::StopWouldTrigger);
        }

        let oid = self.next_order_id();
        let ts_ns = self.market_state.current_timestamp_ns();
        order.stamp_admitted(oid, ts_ns);
        self.pending_stops.try_insert(order)?;
        Ok(order)
    }

    /// Cancel a pending conditional / stop order by [`CancelBy`].
    ///
    /// # Errors
    /// Returns [`StopOrderError::RateLimitReached`] or [`StopOrderError::NotFound`].
    pub fn cancel_stop_order(
        &mut self,
        cancel_by: CancelBy<UserOrderIdT>,
    ) -> Result<StopOrder<I, D, BaseOrQuote, UserOrderIdT>, StopOrderError> {
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        self.pending_stops.remove(cancel_by)
    }

    /// Cancel every pending conditional / stop order on the exchange.
    ///
    /// # Errors
    /// Returns [`StopOrderError::RateLimitReached`] if rate-limited.
    pub fn cancel_all_stop_orders(
        &mut self,
    ) -> Result<Vec<StopOrder<I, D, BaseOrQuote, UserOrderIdT>>, StopOrderError> {
        self.order_rate_limiter
            .aquire(self.market_state.current_ts_ns())?;
        let mut out = Vec::with_capacity(self.pending_stops.len());
        self.pending_stops.drain_all(&mut out);
        Ok(out)
    }

    #[inline]
    fn stop_reference_price(
        &self,
        price_type: TriggerPriceType,
        side: crate::types::Side,
    ) -> Option<QuoteCurrency<I, D>> {
        execution_rules::stop_reference_price(
            &self.market_state,
            self.config.contract_spec().mark_method(),
            price_type,
            side,
        )
    }

    fn evaluate_pending_stops(
        &mut self,
        candle_range: Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>,
    ) {
        self.elected_stops_scratch.clear();
        let mark_method = self.config.contract_spec().mark_method();
        let ms = &self.market_state;

        self.pending_stops.evaluate_and_drain(
            |stop| {
                execution_rules::stop_trigger_band(
                    ms,
                    mark_method,
                    stop.trigger_price_type(),
                    stop.side(),
                    candle_range,
                )
            },
            &mut self.elected_stops_scratch,
        );

        let n_elected = self.elected_stops_scratch.len();
        for i in 0..n_elected {
            let (stop, trigger_price) = self.elected_stops_scratch[i];
            let intrabar =
                execution_rules::elected_intrabar(stop.trigger_price_type(), candle_range);
            self.execute_elected_stop(stop, trigger_price, intrabar);
            if self.liquidated_during_fills {
                core::hint::cold_path();
                // The remaining elected stops were already removed from `pending_stops`,
                // so the liquidation's own stop cancellation does not see them. Report
                // them here, otherwise the user never learns their fate.
                for j in (i + 1)..n_elected {
                    let (left, _) = self.elected_stops_scratch[j];
                    self.push_stop_event(StopOrderEvent::Cancelled {
                        stop_order_id: left.id(),
                        user_order_id: left.user_order_id(),
                        reason: StopCancelReason::ForcedCancel,
                    });
                }
                break;
            }
        }
        self.elected_stops_scratch.clear();
    }

    fn execute_elected_stop(
        &mut self,
        stop: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        trigger_price: QuoteCurrency<I, D>,
        from_candle: bool,
    ) {
        match stop.kind() {
            StopKind::StopMarket | StopKind::TakeProfitMarket | StopKind::TrailingStop { .. } => {
                self.execute_elected_market_stop(stop, trigger_price, from_candle);
            }
            StopKind::StopLimit {
                limit_price,
                time_in_force,
            }
            | StopKind::TakeProfitLimit {
                limit_price,
                time_in_force,
            } => {
                self.execute_elected_limit_stop(stop, trigger_price, limit_price, time_in_force);
            }
        }
    }

    fn execute_elected_market_stop(
        &mut self,
        stop: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        trigger_price: QuoteCurrency<I, D>,
        from_candle: bool,
    ) {
        let exec_qty = if stop.reduce_only() {
            let Some(clamped) = self.clamp_reduce_only_quantity(stop.side(), stop.quantity())
            else {
                self.push_stop_event(StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::ReduceOnlyNoPosition,
                });
                return;
            };
            clamped
        } else {
            stop.quantity()
        };

        let spec = self.config.contract_spec();
        let fill_price = execution_rules::market_stop_fill_price(
            spec.slippage_model(),
            spec.price_filter().tick_size(),
            &self.market_state,
            stop.side(),
            exec_qty,
            stop.stop_price(),
            from_candle,
        );

        // Check StopProtection collar (order override wins over contract default).
        let protection = stop
            .protection()
            .or_else(|| self.config.contract_spec().stop_protection());
        if let Some(collar) = protection
            && !collar.allows_execution(stop.side(), stop.stop_price(), fill_price)
        {
            self.push_stop_event(StopOrderEvent::Cancelled {
                stop_order_id: stop.id(),
                user_order_id: stop.user_order_id(),
                reason: StopCancelReason::StopProtectionBand,
            });
            return;
        }

        let meta = ExchangeOrderMeta::new(stop.id(), self.market_state.current_timestamp_ns());
        let probe =
            MarketOrder::new_with_user_order_id(stop.side(), exec_qty, stop.user_order_id())
                .expect("stop order quantity is strictly positive")
                .into_pending(meta);
        let risk_ok = match self.config.margin_mode() {
            MarginMode::Isolated => self
                .risk_engine
                .check_market_order(&self.account, &probe, fill_price)
                .is_ok(),
            MarginMode::Cross => self
                .cross_risk_engine
                .check_market_order_with_market(
                    &self.account,
                    &self.market_state,
                    &probe,
                    fill_price,
                )
                .is_ok(),
        };
        if !risk_ok {
            self.push_stop_event(StopOrderEvent::Cancelled {
                stop_order_id: stop.id(),
                user_order_id: stop.user_order_id(),
                reason: StopCancelReason::InsufficientMargin,
            });
            return;
        }

        let notional = BaseOrQuote::PairedCurrency::convert_from(exec_qty, fill_price);
        let fee = notional * *self.config.contract_spec().fee_taker().as_ref();
        let eq_before = self.account.balances().equity();
        let bad_debt_before = self.account.balances().bad_debt();
        let realized =
            self.account
                .change_position_realizing(exec_qty, fill_price, stop.side(), fee);
        self.record_ledger_flows(
            Some(stop.id()),
            fill_flows(realized, fee),
            eq_before,
            bad_debt_before,
        );
        let solvency = self.reconcile_margin(bad_debt_before);
        self.drain_forced_cancels_into_events();

        self.push_stop_event(StopOrderEvent::Filled {
            stop_order_id: stop.id(),
            user_order_id: stop.user_order_id(),
            side: stop.side(),
            filled_quantity: exec_qty,
            trigger_price,
            fill_price,
            fee,
            solvency,
        });
        if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
            self.liquidated_during_fills = true;
        }
    }

    fn execute_elected_limit_stop(
        &mut self,
        stop: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        trigger_price: QuoteCurrency<I, D>,
        limit_price: QuoteCurrency<I, D>,
        time_in_force: TimeInForce,
    ) {
        let Ok(child) = LimitOrder::new_with_user_order_id(
            stop.side(),
            limit_price,
            stop.quantity(),
            stop.user_order_id(),
        ) else {
            self.push_stop_event(StopOrderEvent::Cancelled {
                stop_order_id: stop.id(),
                user_order_id: stop.user_order_id(),
                reason: StopCancelReason::ChildLimitRejected,
            });
            return;
        };
        let child = child
            .with_time_in_force(time_in_force)
            .with_reduce_only(stop.reduce_only());

        match self.submit_limit_order_no_rate_limit(child) {
            Ok(LimitOrderSubmission::Resting(pending)) => {
                self.push_stop_event(StopOrderEvent::PlacedLimit {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    child_order_id: pending.id(),
                    trigger_price,
                });
            }
            Ok(LimitOrderSubmission::ImmediatelyFilled {
                filled_order,
                fee,
                forced_cancels,
                solvency,
            }) => {
                // Orders the venue cancelled while settling the child fill must reach the
                // event stream like any other forced cancel of this update.
                for cancelled in forced_cancels {
                    self.limit_order_events
                        .push_within_capacity(LimitOrderEvent::ForcedCancel(cancelled))
                        .expect(EXPECT_CAPACITY);
                }
                self.push_stop_event(StopOrderEvent::Filled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    side: stop.side(),
                    filled_quantity: filled_order.total_quantity(),
                    trigger_price,
                    fill_price: filled_order.state().avg_fill_price(),
                    fee,
                    solvency,
                });
                if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
                    self.liquidated_during_fills = true;
                }
            }
            Err(SubmitLimitOrderError::ReduceOnlyRejected) => {
                self.push_stop_event(StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::ReduceOnlyNoPosition,
                });
            }
            Err(SubmitLimitOrderError::NotEnoughAvailableBalance(_)) => {
                self.push_stop_event(StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::InsufficientMargin,
                });
            }
            Err(_) => {
                self.push_stop_event(StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::ChildLimitRejected,
                });
            }
        }
    }

    /// Clear `stop_order_events` at the start of a public call which can emit stop events.
    ///
    /// A single store. The capacity bound (one event per possible pending stop) is
    /// restored lazily by [`Self::push_stop_event`], the only place that needs it, so the
    /// quote-only update path does not pay for a compare it never takes.
    #[inline]
    fn reset_stop_order_events(&mut self) {
        self.stop_order_events.clear();
    }

    /// Give `stop_order_events` enough spare capacity for the next event.
    ///
    /// `Clone` restores the bound, so only a deserialized exchange (serde cannot restore
    /// spare `Vec` capacity) has to allocate on its first stop event.
    #[cold]
    #[inline(never)]
    fn restore_stop_order_event_capacity(&mut self) {
        let max_stops = usize::from(self.config.max_num_open_orders().get());
        assert!(self.stop_order_events.len() < max_stops);
        self.stop_order_events
            .reserve_exact(max_stops - self.stop_order_events.len());
        assert!(self.stop_order_events.capacity() >= max_stops);
    }

    /// Record a stop event. Cannot overflow: see the `stop_order_events` field docs.
    #[inline]
    fn push_stop_event(&mut self, event: StopOrderEvent<I, D, BaseOrQuote, UserOrderIdT>) {
        let max_stops = usize::from(self.config.max_num_open_orders().get());
        assert2::debug_assert!(self.stop_order_events.len() < max_stops);
        if self.stop_order_events.len() == self.stop_order_events.capacity() {
            core::hint::cold_path();
            self.restore_stop_order_event_capacity();
        }
        self.stop_order_events
            .push_within_capacity(event)
            .expect("each pending stop yields at most one event per call");
    }
}
