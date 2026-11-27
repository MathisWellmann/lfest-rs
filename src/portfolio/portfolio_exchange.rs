//! Core multi-symbol cross-margin [`PortfolioExchange`] implementation.

use std::collections::HashMap;

use const_decimal::Decimal;
use getset::Getters;
use num_traits::Zero;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    debug,
    info,
    warn,
};

use super::{
    DelistSettlement,
    MarketConfig,
    MarketSlotView,
    PendingLimitOrder,
    PortfolioConfig,
    PortfolioError,
    PortfolioFundingSettlement,
    PortfolioMarketOrderSettlement,
    PortfolioOrderEvent,
    PortfolioStopEvent,
    TaggedForcedCancels,
};
use crate::{
    EXPECT_CAPACITY,
    account::{
        ActiveLimitOrders,
        Balances,
        Position,
    },
    contract_specification::ContractSpecification,
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
    market_update::MarketUpdate,
    order_rate_limiter::OrderRateLimiter,
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
        Currency,
        ExchangeOrderMeta,
        FundingRate,
        LimitOrder,
        LimitOrderEvent,
        LimitOrderFill,
        MarginCurrency,
        MarketId,
        MarketOrder,
        MarketStatus,
        MassCancelResult,
        Mon,
        NewOrder,
        NotEnoughAvailableBalance,
        OrderId,
        Pending,
        QuoteCurrency,
        RiskError,
        Side::{
            self,
            *,
        },
        Solvency,
        SubmitLimitOrderError,
        SubmitMarketOrderError,
        TimeInForce,
        TimestampNs,
        UserOrderId,
    },
    utils::reserve_to_capacity,
};

/// Internal per-market slot stored contiguously in [`PortfolioExchange::markets`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, UserOrderIdT: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, UserOrderIdT: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
struct MarketSlot<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    market_id: MarketId,
    status: MarketStatus,
    contract_spec: ContractSpecification<I, D, BaseOrQuote>,
    market_state: MarketState<I, D>,
    position: Position<I, D, BaseOrQuote>,
    active_limit_orders: ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
    pending_stops: PendingStops<I, D, BaseOrQuote, UserOrderIdT>,
    order_rate_limiter: OrderRateLimiter,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> MarketSlot<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    fn new(market_id: MarketId, config: MarketConfig<I, D, BaseOrQuote::PairedCurrency>) -> Self {
        let max_orders = config.max_num_open_orders();
        let order_rate_limiter =
            OrderRateLimiter::new(config.order_rate_limits().orders_per_second());
        Self {
            market_id,
            status: MarketStatus::Active,
            contract_spec: config.contract_spec().clone(),
            market_state: MarketState::default(),
            position: Position::default(),
            active_limit_orders: ActiveLimitOrders::with_capacity(max_orders),
            pending_stops: PendingStops::with_capacity(max_orders),
            order_rate_limiter,
        }
    }

    #[inline]
    fn unrealized_pnl(&self) -> BaseOrQuote::PairedCurrency {
        if self.position.quantity().is_zero() || !self.market_state.has_positive_quotes() {
            return Zero::zero();
        }
        self.position
            .unrealized_pnl_marked(&self.market_state, self.contract_spec.mark_method())
    }

    #[inline]
    fn position_margin(&self) -> BaseOrQuote::PairedCurrency {
        self.position.notional() * self.contract_spec.init_margin_req()
    }

    #[inline]
    fn order_margin(&self) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders
            .order_margin(self.contract_spec.init_margin_req(), &self.position)
    }

    #[inline]
    fn reserved_maker_fees(&self) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders
            .reserved_maker_fees(*self.contract_spec.fee_maker().as_ref())
    }

    #[inline]
    fn required_collateral(&self) -> BaseOrQuote::PairedCurrency {
        self.position_margin() + self.order_margin() + self.reserved_maker_fees()
    }

    #[inline]
    fn maintenance_margin_requirement(&self) -> BaseOrQuote::PairedCurrency {
        if self.position.quantity().is_zero() {
            return Zero::zero();
        }
        let notional = if self.market_state.has_positive_quotes() {
            let mark_price = self
                .market_state
                .mark_price_for_position(self.contract_spec.mark_method(), self.position.side());
            self.position.notional_at_price(mark_price)
        } else {
            self.position.notional()
        };
        self.contract_spec
            .maintenance_margin_schedule()
            .maintenance_margin(notional)
    }

    #[inline]
    fn is_strictly_reducing(&self, side: Side, quantity: BaseOrQuote) -> bool {
        let pos_qty = self.position.quantity();
        match side {
            Buy => pos_qty.is_negative() && quantity <= pos_qty.abs(),
            Sell => pos_qty.is_positive() && quantity <= pos_qty,
        }
    }

    #[inline]
    fn clamp_reduce_only_quantity(&self, side: Side, quantity: BaseOrQuote) -> Option<BaseOrQuote> {
        execution_rules::clamp_reduce_only_quantity(self.position.quantity(), side, quantity)
    }

    #[inline]
    fn largest_collateral_contributor(&self) -> Option<(OrderId, BaseOrQuote::PairedCurrency)> {
        self.active_limit_orders.largest_collateral_contributor(
            self.contract_spec.init_margin_req(),
            &self.position,
            *self.contract_spec.fee_maker().as_ref(),
        )
    }
}

/// Multi-symbol cross-margin perpetual futures exchange.
///
/// All registered markets share a single [`Balances`] wallet. Profits in winning
/// positions offset losses in losing positions under cross-margin, and forced
/// cancellations / liquidations reconcile across the entire portfolio.
#[derive(Debug, Getters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, UserOrderIdT: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, UserOrderIdT: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
pub struct PortfolioExchange<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Shared cross-margin wallet balances.
    #[getset(get = "pub")]
    balances: Balances<I, D, BaseOrQuote::PairedCurrency>,

    /// Dense array of market slots indexed by `MarketId::as_usize()`.
    markets: Vec<MarketSlot<I, D, BaseOrQuote, UserOrderIdT>>,

    /// Cold-path lookup from ticker string to [`MarketId`].
    ticker_index: HashMap<String, MarketId>,

    /// Monotonic order identifier shared across all markets.
    next_order_id: OrderId,

    /// Pre-allocated event buffer populated by [`Self::update_market`].
    #[getset(get = "pub")]
    order_events: Vec<PortfolioOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>,

    /// Conditional / stop order events emitted during the most recent call which can elect
    /// or force-cancel stops: [`Self::update_market`], [`Self::delist_market`], or a
    /// liquidation triggered by [`Self::submit_market_order`],
    /// [`Self::submit_limit_order_with_execution`] or [`Self::apply_funding`]. Each of these
    /// calls clears the buffer first, and its capacity covers every pending stop of every
    /// market (each yields at most one event per call), so it never drops an event.
    #[getset(get = "pub")]
    stop_events: Vec<PortfolioStopEvent<I, D, BaseOrQuote, UserOrderIdT>>,

    /// Sum of the configured pending-stop capacities of all markets, which bounds the
    /// number of stop events one call can emit.
    max_pending_stops: usize,

    /// Scratch buffer for elected stops during `update_market`.
    elected_stops_scratch: Vec<(
        StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        QuoteCurrency<I, D>,
    )>,

    /// Scratch buffer for forced cancellations during reconciliation.
    forced_cancel_scratch: TaggedForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,

    /// Whether a fill-triggered reconciliation liquidated the portfolio during `update_market`.
    liquidated_during_fills: bool,

    /// Optional cashflow / accounting ledger across all markets (disabled by default).
    ledger: Option<Ledger<I, D, BaseOrQuote::PairedCurrency>>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Clone
    for PortfolioExchange<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Clone the portfolio with the pre-allocated buffers of a freshly constructed one.
    ///
    /// `Vec::clone` keeps only the length, so a derived clone left the event and scratch
    /// buffers without spare capacity and the first fill, elected stop or forced cancel
    /// panicked in `push_within_capacity`. `Clone` is cold, so restoring the bounds here
    /// keeps every push path free of capacity checks. The per-market `ActiveLimitOrders`
    /// and `PendingStops` retain their own capacity in their `Clone`.
    fn clone(&self) -> Self {
        let mut cloned = Self {
            balances: self.balances.clone(),
            markets: self.markets.clone(),
            ticker_index: self.ticker_index.clone(),
            next_order_id: self.next_order_id,
            order_events: self.order_events.clone(),
            stop_events: self.stop_events.clone(),
            max_pending_stops: self.max_pending_stops,
            elected_stops_scratch: self.elected_stops_scratch.clone(),
            forced_cancel_scratch: self.forced_cancel_scratch.clone(),
            liquidated_during_fills: self.liquidated_during_fills,
            ledger: self.ledger.clone(),
        };
        cloned.reserve_preallocated_capacities();
        cloned
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> PortfolioExchange<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Construct a new [`PortfolioExchange`] from a [`PortfolioConfig`].
    ///
    /// Markets in `config.markets()` receive dense IDs `MarketId(0..n)` in order.
    #[must_use]
    pub fn new(config: PortfolioConfig<I, D, BaseOrQuote::PairedCurrency>) -> Self {
        let n_markets = config.markets().len();
        // `PortfolioConfig::new` validated both, so these only guard against programmer errors.
        assert!(
            n_markets <= MarketId::COUNT,
            "number of markets exceeds MarketId::COUNT"
        );
        let balances = Balances::new(config.starting_wallet_balance());
        let mut markets = Vec::with_capacity(n_markets);
        let mut ticker_index = HashMap::with_capacity(n_markets);
        let mut total_order_cap: usize = 0;

        for (idx, market_cfg) in config.markets().iter().enumerate() {
            let market_id = MarketId::new(
                u16::try_from(idx).expect("n_markets was asserted <= MarketId::COUNT"),
            );
            total_order_cap += usize::from(market_cfg.max_num_open_orders().get());
            let ticker = market_cfg.contract_spec().ticker();
            if !ticker.is_empty() {
                let previous = ticker_index.insert(ticker.clone(), market_id);
                assert!(
                    previous.is_none(),
                    "PortfolioConfig rejects duplicate tickers"
                );
            }
            markets
                .push_within_capacity(MarketSlot::new(market_id, market_cfg.clone()))
                .expect(EXPECT_CAPACITY);
        }

        let caps = BufferCapacities::for_order_slots(total_order_cap);
        Self {
            balances,
            markets,
            ticker_index,
            next_order_id: OrderId::default(),
            order_events: Vec::with_capacity(caps.order_events()),
            stop_events: Vec::with_capacity(caps.stop_events()),
            max_pending_stops: total_order_cap,
            elected_stops_scratch: Vec::with_capacity(caps.elected_stops()),
            forced_cancel_scratch: Vec::with_capacity(caps.forced_cancels()),
            liquidated_during_fills: false,
            ledger: None,
        }
    }

    /// Give the per-call buffers the capacity [`BufferCapacities`] prescribes for the
    /// current set of markets.
    ///
    /// [`Self::new`] allocates them up front, [`Self::add_market`] grows them to the new
    /// total and [`PortfolioExchange::clone`] restores them after `Vec::clone` dropped
    /// the spare capacity.
    fn reserve_preallocated_capacities(&mut self) {
        // The summed pending-stop capacity is the summed order capacity of all markets.
        let caps = BufferCapacities::for_order_slots(self.max_pending_stops);
        assert_eq!(caps.stop_events(), self.max_pending_stops);
        reserve_to_capacity(&mut self.order_events, caps.order_events());
        reserve_to_capacity(&mut self.stop_events, caps.stop_events());
        reserve_to_capacity(&mut self.elected_stops_scratch, caps.elected_stops());
        reserve_to_capacity(&mut self.forced_cancel_scratch, caps.forced_cancels());
    }

    /// Enable the opt-in portfolio cashflow [`Ledger`] anchored at current wallet equity.
    pub fn enable_ledger(&mut self, capacity: usize) {
        self.ledger = Some(Ledger::with_capacity(self.balances.equity(), capacity));
    }

    /// Borrow the portfolio cashflow [`Ledger`] if enabled.
    #[inline]
    #[must_use]
    pub fn ledger(&self) -> Option<&Ledger<I, D, BaseOrQuote::PairedCurrency>> {
        self.ledger.as_ref()
    }

    /// Journal the nominal `flows` of one settlement which started at `equity_before`.
    ///
    /// The shared wallet is never floored here, so the flows explain the wallet change
    /// exactly. Bad debt is recorded separately by `Self::settle_bad_debt`.
    #[inline]
    fn record_ledger_flows<const N: usize>(
        &mut self,
        ts_ns: TimestampNs,
        market_id: Option<MarketId>,
        order_id: Option<OrderId>,
        flows: [(LedgerEntryKind, BaseOrQuote::PairedCurrency); N],
        equity_before: BaseOrQuote::PairedCurrency,
    ) {
        let Some(ledger) = self.ledger.as_mut() else {
            return;
        };
        let equity_after = ledger.record_flows(ts_ns, market_id, order_id, &flows, equity_before);
        assert_eq!(
            equity_after,
            self.balances.equity(),
            "the journaled cashflows must explain the wallet change exactly"
        );
    }

    /// Dynamically register a new market and return its assigned [`MarketId`].
    ///
    /// # Errors
    /// Returns [`PortfolioError::MaxMarketsReached`] if [`MarketId::COUNT`] markets are
    /// already registered, or [`PortfolioError::DuplicateTicker`] if another market
    /// already uses the (non-empty) ticker, because ticker lookups must stay unambiguous.
    pub fn add_market(
        &mut self,
        market_cfg: MarketConfig<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<MarketId, PortfolioError<UserOrderIdT>> {
        assert!(self.markets.len() <= MarketId::COUNT);
        let Ok(raw_id) = u16::try_from(self.markets.len()) else {
            assert_eq!(self.markets.len(), MarketId::COUNT);
            return Err(PortfolioError::MaxMarketsReached(MarketId::COUNT));
        };
        let market_id = MarketId::new(raw_id);
        let ticker = market_cfg.contract_spec().ticker();
        if !ticker.is_empty() {
            if self.ticker_index.contains_key(ticker) {
                return Err(PortfolioError::DuplicateTicker(ticker.clone()));
            }
            self.ticker_index.insert(ticker.clone(), market_id);
        }
        // `max_pending_stops` is the summed order capacity of all markets, see
        // `BufferCapacities`. Grow every buffer to the new total: a relative
        // `Vec::reserve` only guarantees `len + additional`, and the buffers are empty
        // between calls, so it would not grow them at all.
        self.max_pending_stops += usize::from(market_cfg.max_num_open_orders().get());
        self.reserve_preallocated_capacities();
        #[allow(
            clippy::disallowed_methods,
            reason = "add_market is a cold-path dynamic market registration"
        )]
        self.markets.push(MarketSlot::new(market_id, market_cfg));
        Ok(market_id)
    }

    /// Look up a [`MarketId`] by its contract ticker symbol.
    #[inline]
    #[must_use]
    pub fn market_id_by_ticker(&self, ticker: &str) -> Option<MarketId> {
        self.ticker_index.get(ticker).copied()
    }

    /// Total number of registered markets (including delisted ones).
    #[inline]
    #[must_use]
    pub fn market_count(&self) -> usize {
        self.markets.len()
    }

    /// Return a read-only view of `market_id`, or `None` if out of bounds.
    #[must_use]
    pub fn market(
        &self,
        market_id: MarketId,
    ) -> Option<MarketSlotView<'_, I, D, BaseOrQuote, UserOrderIdT>> {
        let slot = self.markets.get(market_id.as_usize())?;
        Some(MarketSlotView {
            market_id: slot.market_id,
            status: slot.status,
            contract_spec: &slot.contract_spec,
            market_state: &slot.market_state,
            position: &slot.position,
            active_limit_orders: &slot.active_limit_orders,
        })
    }

    /// Return total mark-to-market unrealized PnL summed across all open positions.
    #[must_use]
    pub fn total_unrealized_pnl(&self) -> BaseOrQuote::PairedCurrency {
        let mut total = BaseOrQuote::PairedCurrency::zero();
        for slot in &self.markets {
            total += slot.unrealized_pnl();
        }
        total
    }

    /// Return live cross-margin portfolio equity (`wallet_equity + total_unrealized_pnl`).
    #[must_use]
    pub fn total_equity(&self) -> BaseOrQuote::PairedCurrency {
        self.balances.equity() + self.total_unrealized_pnl()
    }

    /// Return total required initial collateral (positions + resting orders + reserved maker fees)
    /// summed across all markets.
    #[must_use]
    pub fn total_required_collateral(&self) -> BaseOrQuote::PairedCurrency {
        let mut total = BaseOrQuote::PairedCurrency::zero();
        for slot in &self.markets {
            total += slot.required_collateral();
        }
        total
    }

    /// Return total required maintenance margin summed across all open positions.
    #[must_use]
    pub fn total_maintenance_margin(&self) -> BaseOrQuote::PairedCurrency {
        let mut total = BaseOrQuote::PairedCurrency::zero();
        for slot in &self.markets {
            total += slot.maintenance_margin_requirement();
        }
        total
    }

    /// Signed margin excess (`total_equity - total_required_collateral`).
    #[must_use]
    pub fn margin_excess(&self) -> BaseOrQuote::PairedCurrency {
        self.total_equity() - self.total_required_collateral()
    }

    /// Available balance for new risk-increasing orders, floored at zero.
    #[must_use]
    pub fn available_balance(&self) -> BaseOrQuote::PairedCurrency {
        self.margin_excess().max(Zero::zero())
    }

    /// Transition a market between [`MarketStatus::Active`] and [`MarketStatus::ReduceOnly`].
    ///
    /// In reduce-only status only position-reducing orders may fill, so switching to
    /// [`MarketStatus::ReduceOnly`] cancels and returns every resting order on the market
    /// that is not `reduce_only` and could increase or flip the position (see
    /// `ActiveLimitOrders::cancel_risk_increasing`). Switching to `Active` cancels nothing.
    ///
    /// To delist a market and settle any open position, call [`Self::delist_market`].
    ///
    /// # Errors
    /// Returns [`PortfolioError::UnknownMarket`], [`PortfolioError::MarketDelisted`] if
    /// already delisted, or [`PortfolioError::UseDelistMarket`] for
    /// [`MarketStatus::Delisted`], because delisting must settle the open position.
    pub fn set_market_status(
        &mut self,
        market_id: MarketId,
        status: MarketStatus,
    ) -> Result<Vec<PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>>, PortfolioError<UserOrderIdT>>
    {
        let slot = self.slot_mut(market_id)?;
        if slot.status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        if status == MarketStatus::Delisted {
            return Err(PortfolioError::UseDelistMarket(market_id));
        }
        slot.status = status;
        let mut cancelled = Vec::with_capacity(slot.active_limit_orders.len());
        if status == MarketStatus::ReduceOnly {
            slot.active_limit_orders
                .cancel_risk_increasing(slot.position.quantity(), |order| {
                    cancelled
                        .push_within_capacity(order)
                        .expect(EXPECT_CAPACITY);
                });
        }
        assert!(slot.status != MarketStatus::Delisted);
        Ok(cancelled)
    }

    /// Delist `market_id`: cancel every resting limit order on that market,
    /// force-settle any open position at `settlement_price` with zero fee, mark the
    /// market [`MarketStatus::Delisted`], and reconcile portfolio margin.
    ///
    /// # Errors
    /// Returns [`PortfolioError::UnknownMarket`] or [`PortfolioError::MarketDelisted`] if already delisted.
    ///
    /// # Panics
    /// Panics if `settlement_price <= 0`.
    pub fn delist_market(
        &mut self,
        market_id: MarketId,
        settlement_price: QuoteCurrency<I, D>,
    ) -> Result<DelistSettlement<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        assert!(
            settlement_price > QuoteCurrency::zero(),
            "delist settlement price must be strictly positive, got {settlement_price}"
        );
        let idx = self.validate_market_index(market_id)?;
        if self.markets[idx].status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        self.reset_stop_events();

        let cancelled_orders = self.markets[idx].active_limit_orders.cancel_all(None);
        // `take_all` does not allocate when no stop is pending.
        for stop in self.markets[idx].pending_stops.take_all() {
            self.push_stop_event(
                market_id,
                stop.cancelled_event(StopCancelReason::ForcedCancel),
            );
        }
        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        let settled_quantity = self.markets[idx].position.quantity();
        let realized_pnl = if !settled_quantity.is_zero() {
            let close_side = if settled_quantity.is_positive() {
                Sell
            } else {
                Buy
            };
            let pnl = self.book_fill(
                idx,
                settled_quantity.abs(),
                settlement_price,
                close_side,
                Zero::zero(),
            );
            // Fee-free settlement: the wallet moves by exactly the realized pnl. Bad debt,
            // if any, is booked afterwards in `reconcile_portfolio_margin`.
            assert_eq!(self.balances.equity() - eq_before, pnl);
            self.record_ledger_flows(
                ts_ns,
                Some(market_id),
                None,
                [(LedgerEntryKind::DelistSettlement, pnl)],
                eq_before,
            );
            pnl
        } else {
            Zero::zero()
        };
        assert!(self.markets[idx].position.quantity().is_zero());

        self.markets[idx].status = MarketStatus::Delisted;
        let solvency = self.reconcile_portfolio_margin(bad_debt_before, ts_ns);
        let forced_cancels = self.take_forced_cancels();

        Ok(DelistSettlement {
            market_id,
            settled_quantity,
            realized_pnl,
            cancelled_orders,
            forced_cancels,
            solvency,
        })
    }

    /// Feed a [`MarketUpdate`] into `market_id`, run portfolio-wide cross-margin
    /// maintenance checks, and evaluate resting limit orders on `market_id`.
    ///
    /// # Errors
    /// Returns [`PortfolioError::UnknownMarket`] if `market_id` is invalid (nothing is
    /// changed), or [`PortfolioError::Risk`] with [`RiskError::Liquidate`] if the portfolio
    /// fell below maintenance margin (all positions were force-liquidated and resting
    /// orders cancelled, recorded in [`Self::order_events`]).
    pub fn update_market<U>(
        &mut self,
        market_id: MarketId,
        market_update: &U,
    ) -> Result<
        &Vec<PortfolioOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>,
        PortfolioError<UserOrderIdT>,
    >
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        let idx = self.validate_market_index(market_id)?;
        self.order_events.clear();
        self.reset_stop_events();
        self.liquidated_during_fills = false;

        {
            let slot = &mut self.markets[idx];
            let pf = slot.contract_spec.price_filter().clone();
            slot.market_state.update_state(market_update, &pf);
        }

        if !self.markets[idx].pending_stops.is_empty()
            && self.markets[idx].status != MarketStatus::Delisted
        {
            self.evaluate_pending_stops_for_market(idx, market_update.price_range());
            if self.liquidated_during_fills {
                core::hint::cold_path();
                return Err(RiskError::Liquidate.into());
            }
        }

        if self.has_any_open_position() && self.total_equity() < self.total_maintenance_margin() {
            core::hint::cold_path();
            self.force_liquidate_portfolio();
            self.drain_forced_cancels_into_events();
            return Err(RiskError::Liquidate.into());
        }

        if U::CAN_FILL_LIMIT_ORDERS && self.markets[idx].status != MarketStatus::Delisted {
            self.check_active_orders_for_market(idx, market_update.clone());
            if self.liquidated_during_fills {
                core::hint::cold_path();
                return Err(RiskError::Liquidate.into());
            }
        }

        Ok(&self.order_events)
    }

    /// Apply a perpetual funding settlement on `market_id` and reconcile cross-margin
    /// across the entire portfolio.
    ///
    /// # Errors
    /// Returns [`PortfolioError::UnknownMarket`] if `market_id` is invalid.
    ///
    /// # Panics
    /// Panics if `mark_price <= 0`.
    pub fn apply_funding(
        &mut self,
        market_id: MarketId,
        rate: FundingRate<I, D>,
        mark_price: QuoteCurrency<I, D>,
    ) -> Result<
        PortfolioFundingSettlement<I, D, BaseOrQuote, UserOrderIdT>,
        PortfolioError<UserOrderIdT>,
    > {
        let idx = self.validate_market_index(market_id)?;
        let cashflow = self.markets[idx]
            .position
            .funding_cashflow(rate, mark_price);
        // A liquidation reports its stop cancellations in `stop_events`.
        self.reset_stop_events();

        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        if !cashflow.is_zero() {
            self.balances.apply_shared_wallet_funding(cashflow);
            self.record_ledger_flows(
                ts_ns,
                Some(market_id),
                None,
                [(LedgerEntryKind::Funding, cashflow)],
                eq_before,
            );
        }
        let solvency = self.reconcile_portfolio_margin(bad_debt_before, ts_ns);
        let forced_cancels = self.take_forced_cancels();

        Ok(PortfolioFundingSettlement {
            market_id,
            cashflow,
            forced_cancels,
            solvency,
        })
    }

    /// Submit a market order on `market_id`.
    ///
    /// # Errors
    /// Returns [`PortfolioError`] on unknown/delisted/reduce-only market, rate limit,
    /// filter violation, or insufficient cross-margin available balance.
    pub fn submit_market_order(
        &mut self,
        market_id: MarketId,
        mut order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        PortfolioMarketOrderSettlement<I, D, BaseOrQuote, UserOrderIdT>,
        PortfolioError<UserOrderIdT>,
    > {
        let idx = self.validate_market_index(market_id)?;
        let status = self.markets[idx].status;
        if status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        // A liquidation reports its stop cancellations in `stop_events`.
        self.reset_stop_events();

        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(SubmitMarketOrderError::from)?;
        self.markets[idx]
            .contract_spec
            .quantity_filter()
            .validate_order_quantity(order.quantity())
            .map_err(SubmitMarketOrderError::from)?;

        if order.reduce_only() {
            let Some(clamped) =
                self.markets[idx].clamp_reduce_only_quantity(order.side(), order.quantity())
            else {
                return Err(SubmitMarketOrderError::ReduceOnlyRejected.into());
            };
            if clamped < order.quantity() {
                order.set_quantity(clamped);
            }
        }

        let strictly_reducing =
            self.markets[idx].is_strictly_reducing(order.side(), order.quantity());
        if status == MarketStatus::ReduceOnly && !strictly_reducing {
            return Err(PortfolioError::MarketReduceOnly(market_id));
        }

        assert2::debug_assert!(self.markets[idx].market_state.ask() > QuoteCurrency::zero());
        assert2::debug_assert!(self.markets[idx].market_state.bid() > QuoteCurrency::zero());
        let fill_price = self.markets[idx]
            .contract_spec
            .slippage_model()
            .quote(
                &self.markets[idx].market_state,
                order.side(),
                order.quantity(),
                self.markets[idx].contract_spec.price_filter().tick_size(),
            )
            .execution_price();
        execution_rules::validate_order_notional(
            self.markets[idx].contract_spec.notional_filter(),
            order.quantity(),
            fill_price,
            order.reduce_only(),
        )
        .map_err(SubmitMarketOrderError::from)?;

        self.check_portfolio_market_order(idx, order.side(), order.quantity(), fill_price)
            .map_err(SubmitMarketOrderError::from)?;

        let meta = ExchangeOrderMeta::new(self.next_order_id(), ts_ns);
        let filled_order = order.into_pending(meta).into_filled(fill_price, ts_ns);

        let notional =
            BaseOrQuote::PairedCurrency::convert_from(filled_order.quantity(), fill_price);
        let fee = notional * *self.markets[idx].contract_spec.fee_taker().as_ref();

        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let realized = self.book_fill(
            idx,
            filled_order.quantity(),
            fill_price,
            filled_order.side(),
            fee,
        );
        self.record_ledger_flows(
            ts_ns,
            Some(market_id),
            Some(filled_order.state().meta().id()),
            fill_flows(realized, fee),
            eq_before,
        );

        let solvency = self.reconcile_portfolio_margin(bad_debt_before, ts_ns);
        let forced_cancels = self.take_forced_cancels();

        Ok(PortfolioMarketOrderSettlement {
            market_id,
            filled_order,
            forced_cancels,
            solvency,
        })
    }

    /// Submit a passive or marketable limit order on `market_id`, supporting
    /// `TimeInForce` (`GTX`, `GTC`, `IOC`, `FOK`), `RePricing::Marketable`, and `reduce_only`.
    ///
    /// A liquidation caused by an immediate fill reports the cancelled stop orders in
    /// [`Self::stop_events`], which this call clears first.
    ///
    /// # Errors
    /// Returns [`PortfolioError`] on unknown/delisted/reduce-only market, rate limit,
    /// filter violation, crossing GTX order, capacity limit, or insufficient cross-margin.
    pub fn submit_limit_order_with_execution(
        &mut self,
        market_id: MarketId,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        crate::exchange::LimitOrderSubmission<I, D, BaseOrQuote, UserOrderIdT>,
        PortfolioError<UserOrderIdT>,
    > {
        self.acquire_submit_rate_limit(market_id)?;
        self.reset_stop_events();
        self.submit_limit_order_no_rate_limit(market_id, order)
    }

    /// Acquire a user rate-limit slot on `market_id` for a limit order submission.
    ///
    /// # Errors
    /// [`PortfolioError::UnknownMarket`], [`PortfolioError::MarketDelisted`] or the rate
    /// limit error, checked in this order so they match the order of the admission checks.
    fn acquire_submit_rate_limit(
        &mut self,
        market_id: MarketId,
    ) -> Result<(), PortfolioError<UserOrderIdT>> {
        let idx = self.validate_market_index(market_id)?;
        if self.markets[idx].status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(SubmitLimitOrderError::from)?;
        Ok(())
    }

    /// [`Self::submit_limit_order_with_execution`] without the user's rate limiter and
    /// without clearing `stop_events`.
    fn submit_limit_order_no_rate_limit(
        &mut self,
        market_id: MarketId,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        crate::exchange::LimitOrderSubmission<I, D, BaseOrQuote, UserOrderIdT>,
        PortfolioError<UserOrderIdT>,
    > {
        assert!(self.forced_cancel_scratch.is_empty());
        let mut submission = self.submit_limit_order_keep_cancels(market_id, order);
        if let Ok(crate::exchange::LimitOrderSubmission::ImmediatelyFilled {
            forced_cancels, ..
        }) = &mut submission
        {
            assert!(forced_cancels.is_empty());
            let tagged_cancels = self.take_forced_cancels();
            forced_cancels.reserve_exact(tagged_cancels.len());
            for (_, o) in tagged_cancels {
                forced_cancels
                    .push_within_capacity(o)
                    .expect(EXPECT_CAPACITY);
            }
        }
        assert!(self.forced_cancel_scratch.is_empty());
        submission
    }

    /// Like [`Self::submit_limit_order_with_execution`], but an immediate fill leaves its
    /// forced cancellations in `forced_cancel_scratch` (with their market tags, as a
    /// margin call may cancel orders on other markets) and returns an empty
    /// `forced_cancels`. Internal callers route the scratch into the event stream.
    ///
    /// Does not acquire the user's order rate limiter: public callers do that first, and
    /// elected stop-limit children placed inside `update_market` must not be rejected for
    /// it, as the user already paid a rate-limit slot when submitting the stop.
    #[allow(
        clippy::too_many_lines,
        reason = "keeping all TIF and reduce-only admission branches in one place preserves clarity"
    )]
    fn submit_limit_order_keep_cancels(
        &mut self,
        market_id: MarketId,
        mut order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<
        crate::exchange::LimitOrderSubmission<I, D, BaseOrQuote, UserOrderIdT>,
        PortfolioError<UserOrderIdT>,
    > {
        let idx = self.validate_market_index(market_id)?;
        let status = self.markets[idx].status;
        if status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }

        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .contract_spec
            .quantity_filter()
            .validate_order_quantity(order.remaining_quantity())
            .map_err(SubmitLimitOrderError::from)?;

        let mark_price = self.markets[idx]
            .market_state
            .mark_price(self.markets[idx].contract_spec.mark_method());
        self.markets[idx]
            .contract_spec
            .price_filter()
            .validate_limit_price(order.limit_price(), mark_price)
            .map_err(SubmitLimitOrderError::from)?;

        if order.reduce_only() {
            execution_rules::clamp_reduce_only_limit_order(
                &mut order,
                self.markets[idx].position.quantity(),
            )?;
        }

        let strictly_reducing =
            self.markets[idx].is_strictly_reducing(order.side(), order.remaining_quantity());
        if status == MarketStatus::ReduceOnly && !strictly_reducing {
            return Err(PortfolioError::MarketReduceOnly(market_id));
        }
        let takes_liquidity = order.would_take_liquidity(&self.markets[idx].market_state);
        // An executing order is validated at its fill price, a resting one at its limit.
        let notional_price = if takes_liquidity {
            execution_rules::marketable_limit_fill_price(
                self.markets[idx].contract_spec.slippage_model(),
                self.markets[idx].contract_spec.price_filter().tick_size(),
                &self.markets[idx].market_state,
                order.side(),
                order.remaining_quantity(),
                order.limit_price(),
            )
        } else {
            order.limit_price()
        };
        execution_rules::validate_order_notional(
            self.markets[idx].contract_spec.notional_filter(),
            order.remaining_quantity(),
            notional_price,
            order.reduce_only(),
        )
        .map_err(SubmitLimitOrderError::from)?;

        if !takes_liquidity && order.is_marketable(&self.markets[idx].market_state) {
            assert!(order.is_post_only());
            let meta = ExchangeOrderMeta::new(self.next_order_id(), ts_ns);
            let pending = order.into_pending(meta);
            self.check_portfolio_limit_order(idx, &pending)
                .map_err(SubmitLimitOrderError::from)?;
            return Err(SubmitLimitOrderError::GoodTillCrossingRejectedOrder {
                limit_price: pending.limit_price().to_string(),
                away_market_quotation_price: match pending.side() {
                    Buy => self.markets[idx].market_state.ask().to_string(),
                    Sell => self.markets[idx].market_state.bid().to_string(),
                },
            }
            .into());
        }

        if takes_liquidity {
            let fill_price = notional_price;
            self.check_portfolio_market_order(
                idx,
                order.side(),
                order.remaining_quantity(),
                fill_price,
            )
            .map_err(SubmitLimitOrderError::from)?;

            let meta = ExchangeOrderMeta::new(self.next_order_id(), ts_ns);
            let pending = order.into_pending(meta);
            let filled_qty = pending.remaining_quantity();
            let notional = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price);
            let fee = notional * *self.markets[idx].contract_spec.fee_taker().as_ref();
            let eq_before = self.balances.equity();
            let bad_debt_before = self.balances.bad_debt();
            let realized = self.book_fill(idx, filled_qty, fill_price, pending.side(), fee);
            self.record_ledger_flows(
                ts_ns,
                Some(market_id),
                Some(pending.id()),
                fill_flows(realized, fee),
                eq_before,
            );
            let solvency = self.reconcile_portfolio_margin(bad_debt_before, ts_ns);
            let filled_order = pending.into_filled_at_price(fill_price, ts_ns);
            return Ok(crate::exchange::LimitOrderSubmission::ImmediatelyFilled {
                filled_order,
                fee,
                // Left in `forced_cancel_scratch`, see the doc comment.
                forced_cancels: Vec::with_capacity(0),
                solvency,
            });
        }

        if matches!(
            order.time_in_force(),
            TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill
        ) {
            return Err(SubmitLimitOrderError::ImmediateExecutionNotMarketable.into());
        }

        if order.reduce_only() {
            let slot = &self.markets[idx];
            let resting = slot.active_limit_orders.reduce_only_remaining(order.side());
            order.clamp_to_reduce_only_room(slot.position.quantity(), resting)?;
        }

        let meta = ExchangeOrderMeta::new(self.next_order_id(), ts_ns);
        let pending = order.into_pending(meta);
        self.check_portfolio_limit_order(idx, &pending)
            .map_err(SubmitLimitOrderError::from)?;

        self.markets[idx]
            .active_limit_orders
            .try_insert(pending.clone())
            .map_err(SubmitLimitOrderError::from)?;
        Ok(crate::exchange::LimitOrderSubmission::Resting(pending))
    }

    /// Submit a passive limit order on `market_id`.
    ///
    /// This API never trades: an order which would execute immediately (it crosses the
    /// spread and is not post-only) is rejected with
    /// [`SubmitLimitOrderError::WouldExecuteImmediately`] without touching the portfolio,
    /// because the returned pending order could not report the fill, fee, forced cancels
    /// or solvency. Use [`Self::submit_limit_order_with_execution`] for marketable orders.
    ///
    /// # Errors
    /// Returns [`PortfolioError`] on unknown/delisted/reduce-only market, rate limit,
    /// filter violation, crossing order, capacity limit, or insufficient cross-margin.
    pub fn submit_limit_order(
        &mut self,
        market_id: MarketId,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        let idx = self.validate_market_index(market_id)?;
        // Checked before the rate limiter, so such a rejection costs no slot.
        if order.would_take_liquidity(&self.markets[idx].market_state) {
            return Err(SubmitLimitOrderError::WouldExecuteImmediately.into());
        }
        // A resting order never settles a fill, so it cannot liquidate or emit stop events.
        self.acquire_submit_rate_limit(market_id)?;
        self.rest_limit_order_no_rate_limit(market_id, order)
    }

    /// [`Self::submit_limit_order`] without the user's rate limiter.
    fn rest_limit_order_no_rate_limit(
        &mut self,
        market_id: MarketId,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        let idx = self.validate_market_index(market_id)?;
        if order.would_take_liquidity(&self.markets[idx].market_state) {
            return Err(SubmitLimitOrderError::WouldExecuteImmediately.into());
        }
        match self.submit_limit_order_no_rate_limit(market_id, order)? {
            crate::exchange::LimitOrderSubmission::Resting(pending) => {
                assert!(pending.remaining_quantity() > BaseOrQuote::zero());
                Ok(pending)
            }
            crate::exchange::LimitOrderSubmission::ImmediatelyFilled { .. } => {
                unreachable!("marketable orders are rejected before submission")
            }
        }
    }

    /// Cancel an active limit order on `market_id`.
    ///
    /// # Errors
    /// Returns [`PortfolioError`] if `market_id` is invalid, rate-limited, or order not found.
    pub fn cancel_limit_order(
        &mut self,
        market_id: MarketId,
        cancel_by: CancelBy<UserOrderIdT>,
    ) -> Result<PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        let idx = self.validate_market_index(market_id)?;
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(crate::types::CancelLimitOrderError::from)?;
        let removed = self.markets[idx]
            .active_limit_orders
            .remove_limit_order(cancel_by)
            .map_err(crate::types::CancelLimitOrderError::from)?;
        Ok(removed)
    }

    /// Amend an active limit order on `market_id` (CBOE leaves-qty semantics).
    ///
    /// # Errors
    /// Returns [`PortfolioError`] if the market is invalid/delisted, order is not active,
    /// amended quantity was already filled, or re-submission fails.
    pub fn amend_limit_order(
        &mut self,
        market_id: MarketId,
        existing_order_id: OrderId,
        mut new_order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        let idx = self.validate_market_index(market_id)?;
        if self.markets[idx].status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(AmendLimitOrderError::from)?;

        let existing = self.markets[idx]
            .active_limit_orders
            .get_by_id(existing_order_id, new_order.side())
            .ok_or_else(|| {
                AmendLimitOrderError::order_not_on_book(existing_order_id, self.next_order_id)
            })?;

        let Some(new_leaves_qty) = existing.amended_leaves_qty(&new_order) else {
            self.markets[idx]
                .active_limit_orders
                .remove_limit_order(CancelBy::OrderId(existing_order_id))
                .expect("existing active order can be cancelled");
            return Err(AmendLimitOrderError::AmendQtyAlreadyFilled.into());
        };
        new_order.set_remaining_quantity(new_leaves_qty);
        let original = self.markets[idx]
            .active_limit_orders
            .remove_limit_order(CancelBy::OrderId(existing_order_id))
            .expect("existing active order can be cancelled");
        // The admission checks must see the book without the original order, so it is
        // removed first and restored (same id and queue priority) if the replacement fails.
        // The amend already paid its rate-limit slot above.
        let side = new_order.side();
        // Like `Exchange::amend_limit_order`, a rejected replacement is an amend error.
        let result = self
            .rest_limit_order_no_rate_limit(market_id, new_order)
            .map_err(|e| match e {
                PortfolioError::SubmitLimitOrder(e) => AmendLimitOrderError::from(e).into(),
                other => other,
            });
        if result.is_err() {
            self.markets[idx]
                .active_limit_orders
                .try_insert(original)
                .expect("the slot of the original order was just freed");
            assert!(
                self.markets[idx]
                    .active_limit_orders
                    .get_by_id(existing_order_id, side)
                    .is_some()
            );
        }
        result
    }

    /// Cancel all active limit orders on a single `market_id` (optionally filtered by `side`).
    ///
    /// # Errors
    /// Returns [`PortfolioError::UnknownMarket`] or [`PortfolioError::RateLimitReached`].
    pub fn cancel_all_limit_orders(
        &mut self,
        market_id: MarketId,
        side: Option<Side>,
    ) -> Result<MassCancelResult<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        let idx = self.validate_market_index(market_id)?;
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx].order_rate_limiter.aquire(ts_ns)?;
        let cancelled = self.markets[idx].active_limit_orders.cancel_all(side);
        Ok(MassCancelResult::new(cancelled))
    }

    /// Cancel every active limit order across all markets in deterministic `MarketId` order.
    #[must_use]
    pub fn cancel_all_across_markets(
        &mut self,
    ) -> TaggedForcedCancels<I, D, BaseOrQuote, UserOrderIdT> {
        let total: usize = self
            .markets
            .iter()
            .map(|slot| slot.active_limit_orders.len())
            .sum();
        let mut out = Vec::with_capacity(total);
        for slot in &mut self.markets {
            let market_id = slot.market_id;
            for order in slot.active_limit_orders.cancel_all(None) {
                out.push_within_capacity((market_id, order))
                    .expect(EXPECT_CAPACITY);
            }
        }
        out
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> OrderId {
        let oid = self.next_order_id;
        self.next_order_id.incr();
        oid
    }

    #[inline]
    fn validate_market_index(
        &self,
        market_id: MarketId,
    ) -> Result<usize, PortfolioError<UserOrderIdT>> {
        let idx = market_id.as_usize();
        if idx >= self.markets.len() {
            return Err(PortfolioError::UnknownMarket(market_id));
        }
        Ok(idx)
    }

    #[inline]
    fn slot_mut(
        &mut self,
        market_id: MarketId,
    ) -> Result<&mut MarketSlot<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        let idx = self.validate_market_index(market_id)?;
        Ok(&mut self.markets[idx])
    }

    #[inline]
    fn has_any_open_position(&self) -> bool {
        self.markets
            .iter()
            .any(|slot| !slot.position.quantity().is_zero())
    }

    fn check_portfolio_market_order(
        &self,
        idx: usize,
        side: Side,
        quantity: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
    ) -> Result<(), NotEnoughAvailableBalance> {
        let slot = &self.markets[idx];
        let pos_qty = slot.position.quantity();
        let same_direction = match side {
            Buy => pos_qty >= Zero::zero(),
            Sell => pos_qty <= Zero::zero(),
        };
        if same_direction {
            let notional = BaseOrQuote::PairedCurrency::convert_from(quantity, fill_price);
            let init_margin = notional * slot.contract_spec.init_margin_req();
            let fee = notional * *slot.contract_spec.fee_taker().as_ref();
            if init_margin + fee > self.available_balance() {
                return Err(NotEnoughAvailableBalance);
            }
            return Ok(());
        }

        let abs_qty = pos_qty.abs();
        if quantity <= abs_qty {
            // Strictly position-reducing order is never rejected.
            return Ok(());
        }

        // Position flip: compute post-trade portfolio equity vs total initial margin,
        // including how the flipped position changes the resting order margin on this market.
        let flip_qty = quantity - abs_qty;
        let total_notional = BaseOrQuote::PairedCurrency::convert_from(quantity, fill_price);
        let flip_notional = BaseOrQuote::PairedCurrency::convert_from(flip_qty, fill_price);
        let fee = total_notional * *slot.contract_spec.fee_taker().as_ref();
        let realized_pnl =
            BaseOrQuote::PairedCurrency::pnl(slot.position.entry_price(), fill_price, pos_qty);
        let mut post_position = slot.position.clone();
        let _ = post_position.change_realizing(quantity, fill_price, side);
        let post_order_margin = slot
            .active_limit_orders
            .order_margin(slot.contract_spec.init_margin_req(), &post_position);

        // Remove old market's uPnL, position margin, and order margin; add realized_pnl - fee,
        // the flip position margin, and the post-flip resting order margin.
        let other_upnl = self.total_unrealized_pnl() - slot.unrealized_pnl();
        let other_req =
            self.total_required_collateral() - slot.position_margin() - slot.order_margin();
        let post_equity = self.balances.equity() + realized_pnl - fee + other_upnl;
        let post_req =
            other_req + flip_notional * slot.contract_spec.init_margin_req() + post_order_margin;
        if post_equity < post_req {
            return Err(NotEnoughAvailableBalance);
        }
        Ok(())
    }

    fn check_portfolio_limit_order(
        &self,
        idx: usize,
        pending: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), NotEnoughAvailableBalance> {
        let slot = &self.markets[idx];
        let im = slot.contract_spec.init_margin_req();
        let maker_fee = (*slot.contract_spec.fee_maker().as_ref()).max(Decimal::zero());
        let old_order_margin = slot.order_margin();
        let new_order_margin =
            slot.active_limit_orders
                .order_margin_with_order(pending, im, &slot.position);
        let extra_fee_reserve = pending.notional() * maker_fee;
        let extra_req = (new_order_margin - old_order_margin) + extra_fee_reserve;
        if self.margin_excess() < extra_req {
            return Err(NotEnoughAvailableBalance);
        }
        Ok(())
    }

    fn check_active_orders_for_market<U>(&mut self, idx: usize, mut market_update: U)
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        if market_update.can_fill_bids() {
            while let Some(order) = self.markets[idx].active_limit_orders.best_bid().cloned() {
                if let Some((filled_qty, exhausted)) = market_update.limit_order_filled(&order) {
                    if self.apply_limit_fill(
                        idx,
                        order,
                        filled_qty,
                        market_update.timestamp_exchange_ns(),
                    ) {
                        return;
                    }
                    if exhausted {
                        // Only this side's taker flow is used up; a two-sided update
                        // (e.g. `SmartCandle`) may still fill the other side.
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        if market_update.can_fill_asks() {
            while let Some(order) = self.markets[idx].active_limit_orders.best_ask().cloned() {
                if let Some((filled_qty, exhausted)) = market_update.limit_order_filled(&order) {
                    if self.apply_limit_fill(
                        idx,
                        order,
                        filled_qty,
                        market_update.timestamp_exchange_ns(),
                    ) {
                        return;
                    }
                    if exhausted {
                        // Only this side's taker flow is used up; a two-sided update
                        // (e.g. `SmartCandle`) may still fill the other side.
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    /// Apply one resting limit fill on `idx`, reconcile cross-margin, and return `true`
    /// if the portfolio was liquidated or bankrupted.
    fn apply_limit_fill(
        &mut self,
        idx: usize,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        filled_quantity: BaseOrQuote,
        ts_ns: TimestampNs,
    ) -> bool {
        let side = order.side();
        // Defence in depth: `reconcile_portfolio_margin` keeps resting reduce-only orders
        // (and every order on a reduce-only market) within the position, but such a fill
        // must never increase or flip it anyway.
        let filled_quantity =
            if order.reduce_only() || self.markets[idx].status == MarketStatus::ReduceOnly {
                self.markets[idx]
                    .clamp_reduce_only_quantity(side, filled_quantity)
                    .expect("risk-increasing resting orders are pruned after every position change")
            } else {
                filled_quantity
            };
        assert2::debug_assert!(filled_quantity > BaseOrQuote::zero());
        let limit_price = order.limit_price();
        let market_id = self.markets[idx].market_id;
        debug!(
            "portfolio market {market_id}: filled limit {side} order {}: {filled_quantity}/{} @ {limit_price}",
            order.id(),
            order.remaining_quantity(),
        );

        let notional = BaseOrQuote::PairedCurrency::convert_from(filled_quantity, limit_price);
        let fee = notional * *self.markets[idx].contract_spec.fee_maker().as_ref();
        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();

        let realized = self.book_fill(idx, filled_quantity, limit_price, side, fee);
        self.record_ledger_flows(
            ts_ns,
            Some(market_id),
            Some(order.id()),
            fill_flows(realized, fee),
            eq_before,
        );

        let fill_event =
            match self.markets[idx]
                .active_limit_orders
                .fill_best(side, filled_quantity, ts_ns)
            {
                Some(order_after_fill) => LimitOrderFill::FullyFilled {
                    filled_quantity,
                    fee,
                    order_after_fill,
                },
                None => LimitOrderFill::PartiallyFilled {
                    filled_quantity,
                    fee,
                    order_after_fill: self.markets[idx]
                        .active_limit_orders
                        .get_by_id(order.id(), side)
                        .cloned()
                        .expect("partially filled order remains active"),
                },
            };

        self.order_events
            .push_within_capacity(PortfolioOrderEvent {
                market_id,
                event: LimitOrderEvent::Fill(fill_event),
            })
            .expect(EXPECT_CAPACITY);

        let solvency = self.reconcile_portfolio_margin(bad_debt_before, ts_ns);
        self.drain_forced_cancels_into_events();
        if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
            core::hint::cold_path();
            self.liquidated_during_fills = true;
            return true;
        }
        false
    }

    /// Force-liquidate the entire cross-margin portfolio: cancel every resting order across
    /// all markets, then close every non-zero position at its current bid/ask charging
    /// `fee_taker`, and finally charge the summed `liquidation_fee` capped at the equity
    /// the closed portfolio has left.
    fn force_liquidate_portfolio(&mut self) {
        warn!(
            "liquidating cross-margin portfolio: equity={}, mm={}",
            self.total_equity(),
            self.total_maintenance_margin()
        );
        for slot in &mut self.markets {
            let market_id = slot.market_id;
            for cancelled in slot.active_limit_orders.cancel_all(None) {
                self.forced_cancel_scratch
                    .push_within_capacity((market_id, cancelled))
                    .expect(EXPECT_CAPACITY);
            }
            for stop in slot.pending_stops.take_all() {
                self.stop_events
                    .push_within_capacity(PortfolioStopEvent {
                        market_id,
                        event: stop.cancelled_event(StopCancelReason::ForcedCancel),
                    })
                    .expect("each pending stop yields at most one event per call");
            }
        }
        let n_markets = self.markets.len();
        let mut clearance_fee = BaseOrQuote::PairedCurrency::zero();
        for idx in 0..n_markets {
            let pos_qty = self.markets[idx].position.quantity();
            if pos_qty.is_zero() {
                continue;
            }
            let market_id = self.markets[idx].market_id;
            let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
            let (side, fill_price) = if pos_qty.is_negative() {
                let p = if self.markets[idx].market_state.ask() > QuoteCurrency::zero() {
                    self.markets[idx].market_state.ask()
                } else {
                    self.markets[idx].position.entry_price()
                };
                (Buy, p)
            } else {
                let p = if self.markets[idx].market_state.bid() > QuoteCurrency::zero() {
                    self.markets[idx].market_state.bid()
                } else {
                    self.markets[idx].position.entry_price()
                };
                (Sell, p)
            };
            let qty = pos_qty.abs();
            let notional = BaseOrQuote::PairedCurrency::convert_from(qty, fill_price);
            let fee = notional * *self.markets[idx].contract_spec.fee_taker().as_ref();
            clearance_fee += notional * self.markets[idx].contract_spec.liquidation_fee();
            let eq_before = self.balances.equity();
            // Booked without a bankruptcy floor so the net result of closing every leg
            // does not depend on the order in which the legs are closed.
            let realized = self.book_fill(idx, qty, fill_price, side, fee);
            self.record_ledger_flows(
                ts_ns,
                Some(market_id),
                None,
                [
                    (LedgerEntryKind::Liquidation, realized),
                    (LedgerEntryKind::TradingFee, -fee),
                ],
                eq_before,
            );
        }
        assert!(
            !self.has_any_open_position(),
            "portfolio liquidation must close every position"
        );
        // All legs are closed, so the wallet is all that is left of the portfolio:
        // net the combined result against it once.
        let ts_ns = self
            .markets
            .iter()
            .map(|slot| slot.market_state.current_timestamp_ns())
            .max()
            .unwrap_or_default();
        // The clearance fee is collected only from the equity the closed portfolio has
        // left, so the fee alone never turns into bad debt the venue absorbs itself.
        let clearance_fee =
            execution_rules::capped_clearance_fee(clearance_fee, self.balances.equity());
        if !clearance_fee.is_zero() {
            let eq_before = self.balances.equity();
            self.balances.account_for_shared_wallet_fee(clearance_fee);
            assert!(self.balances.equity() >= Zero::zero());
            self.record_ledger_flows(
                ts_ns,
                None,
                None,
                [(LedgerEntryKind::Liquidation, -clearance_fee)],
                eq_before,
            );
        }
        self.settle_bad_debt(ts_ns, None);
        assert!(self.balances.equity() >= Zero::zero());
        info!("balances after portfolio liquidation: {}", self.balances);
    }

    /// Apply a fill on market `idx` to its position and the shared wallet, returning the
    /// realized pnl (excluding `fee`).
    ///
    /// The shared wallet is not floored at zero here: a loss on one leg may be covered by
    /// unrealized profit on the others. Bad debt is only booked by
    /// `Self::settle_bad_debt` once the portfolio is actually insolvent.
    #[inline]
    fn book_fill(
        &mut self,
        idx: usize,
        quantity: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        fee: BaseOrQuote::PairedCurrency,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(quantity > BaseOrQuote::zero());
        assert2::debug_assert!(fill_price > QuoteCurrency::zero());
        let realized = self.markets[idx]
            .position
            .change_realizing(quantity, fill_price, side);
        self.balances.apply_shared_wallet_pnl(realized);
        self.balances.account_for_shared_wallet_fee(fee);
        realized
    }

    /// Move a negative shared wallet into bad debt (absorbed by the venue) and record it.
    ///
    /// Only valid once no position is open, because otherwise unrealized profit on an
    /// open leg could still cover the deficit.
    fn settle_bad_debt(&mut self, ts_ns: TimestampNs, market_id: Option<MarketId>) {
        assert!(
            !self.has_any_open_position(),
            "bad debt can only be settled once every position is closed"
        );
        let absorbed = self.balances.settle_negative_equity();
        assert!(absorbed >= Zero::zero());
        if absorbed.is_zero() {
            return;
        }
        core::hint::cold_path();
        if let Some(ledger) = self.ledger.as_mut() {
            // A positive delta keeps `starting_balance + net_cashflow == equity` exact:
            // the venue credits the wallet back to zero.
            ledger.record(
                ts_ns,
                market_id,
                None,
                LedgerEntryKind::BadDebtAbsorption,
                absorbed,
                self.balances.equity(),
            );
        }
    }

    /// Liquidate the portfolio if it fell below maintenance margin, book bad debt if the
    /// portfolio is insolvent, force-cancel orders until initial margin is covered, and
    /// report the resulting [`Solvency`] relative to `bad_debt_before`.
    #[must_use]
    fn reconcile_portfolio_margin(
        &mut self,
        bad_debt_before: BaseOrQuote::PairedCurrency,
        ts_ns: TimestampNs,
    ) -> Solvency {
        let liquidated = if self.has_any_open_position()
            && self.total_equity() < self.total_maintenance_margin()
        {
            core::hint::cold_path();
            self.force_liquidate_portfolio();
            true
        } else {
            if !self.has_any_open_position() {
                // Closing the last leg can leave the shared wallet negative; with no
                // unrealized profit left to cover it the portfolio is insolvent.
                self.settle_bad_debt(ts_ns, None);
            }
            self.prune_stale_reduce_only_orders();
            false
        };
        // With open positions above maintenance margin, total equity is non-negative
        // even if the shared wallet itself is temporarily negative.
        assert2::debug_assert!(self.total_equity() >= Zero::zero());
        assert2::debug_assert!(self.balances.bad_debt() >= bad_debt_before);

        while self.margin_excess() < Zero::zero() {
            core::hint::cold_path();
            let victim = self
                .markets
                .iter()
                .enumerate()
                .filter_map(|(idx, slot)| {
                    slot.largest_collateral_contributor()
                        .map(|(oid, delta)| (idx, slot.market_id, oid, delta))
                })
                .max_by_key(|&(_, _, _, delta)| delta);
            let Some((idx, market_id, victim_id, _)) = victim else {
                break;
            };
            let cancelled = self.markets[idx]
                .active_limit_orders
                .remove_limit_order(CancelBy::OrderId(victim_id))
                .expect("victim order id belongs to an active order");
            warn!(
                "portfolio margin call: force-cancelling order {} on {market_id}",
                cancelled.id()
            );
            self.forced_cancel_scratch
                .push_within_capacity((market_id, cancelled))
                .expect(EXPECT_CAPACITY);
        }

        if self.balances.bad_debt() > bad_debt_before {
            core::hint::cold_path();
            Solvency::Bankrupt
        } else if liquidated {
            core::hint::cold_path();
            Solvency::Liquidated
        } else if self.margin_excess() < Zero::zero() {
            core::hint::cold_path();
            Solvency::InitialMarginDeficit
        } else {
            Solvency::Solvent
        }
    }

    /// Force-cancel resting `reduce_only` limit orders which no longer fit within their
    /// market's open position, see `ActiveLimitOrders::cancel_excess_reduce_only`, and on
    /// markets in [`MarketStatus::ReduceOnly`] every other order which could now increase
    /// the position, see `ActiveLimitOrders::cancel_risk_increasing`.
    /// Costs two branches per active market without resting reduce-only orders.
    fn prune_stale_reduce_only_orders(&mut self) {
        let scratch = &mut self.forced_cancel_scratch;
        for slot in &mut self.markets {
            let market_id = slot.market_id;
            let mut on_cancel = |cancelled| {
                scratch
                    .push_within_capacity((market_id, cancelled))
                    .expect(EXPECT_CAPACITY);
            };
            let pos_qty = slot.position.quantity();
            slot.active_limit_orders
                .cancel_excess_reduce_only(pos_qty, &mut on_cancel);
            if slot.status == MarketStatus::ReduceOnly {
                slot.active_limit_orders
                    .cancel_risk_increasing(pos_qty, &mut on_cancel);
            }
        }
    }

    fn drain_forced_cancels_into_events(&mut self) {
        for i in 0..self.forced_cancel_scratch.len() {
            let (market_id, order) = self.forced_cancel_scratch[i].clone();
            self.order_events
                .push_within_capacity(PortfolioOrderEvent {
                    market_id,
                    event: LimitOrderEvent::ForcedCancel(order),
                })
                .expect(EXPECT_CAPACITY);
        }
        self.forced_cancel_scratch.clear();
    }

    fn take_forced_cancels(&mut self) -> TaggedForcedCancels<I, D, BaseOrQuote, UserOrderIdT> {
        let mut out = Vec::with_capacity(self.forced_cancel_scratch.len());
        for entry in self.forced_cancel_scratch.drain(..) {
            out.push_within_capacity(entry).expect(EXPECT_CAPACITY);
        }
        out
    }

    /// Submit a conditional / stop order on `market_id`.
    ///
    /// # Errors
    /// Returns [`PortfolioError`] if `market_id` is unknown/delisted, rate-limited,
    /// invalid quantity, immediately triggered (`StopWouldTrigger`), or capacity full.
    pub fn submit_stop_order(
        &mut self,
        market_id: MarketId,
        mut order: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
    ) -> Result<StopOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>> {
        let idx = self.validate_market_index(market_id)?;
        if self.markets[idx].status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(StopOrderError::from)?;
        self.markets[idx]
            .contract_spec
            .quantity_filter()
            .validate_order_quantity(order.quantity())
            .map_err(StopOrderError::from)?;

        if let Some(ref_price) =
            self.stop_reference_price_for_slot(idx, order.trigger_price_type(), order.side())
            && order.is_triggered_by(ref_price)
        {
            return Err(StopOrderError::StopWouldTrigger.into());
        }

        let oid = self.next_order_id();
        order.stamp_admitted(oid, ts_ns);
        self.markets[idx].pending_stops.try_insert(order)?;
        Ok(order)
    }

    /// Cancel a pending conditional / stop order on `market_id`.
    ///
    /// # Errors
    /// Returns [`PortfolioError`] on unknown market, rate limit, or not found.
    pub fn cancel_stop_order(
        &mut self,
        market_id: MarketId,
        cancel_by: CancelBy<UserOrderIdT>,
    ) -> Result<StopOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>> {
        let idx = self.validate_market_index(market_id)?;
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(StopOrderError::from)?;
        let removed = self.markets[idx].pending_stops.remove(cancel_by)?;
        Ok(removed)
    }

    /// Inspect pending stop orders on `market_id`.
    #[must_use]
    pub fn pending_stops(
        &self,
        market_id: MarketId,
    ) -> Option<&PendingStops<I, D, BaseOrQuote, UserOrderIdT>> {
        self.markets
            .get(market_id.as_usize())
            .map(|slot| &slot.pending_stops)
    }

    #[inline]
    fn stop_reference_price_for_slot(
        &self,
        idx: usize,
        price_type: TriggerPriceType,
        side: Side,
    ) -> Option<QuoteCurrency<I, D>> {
        let slot = &self.markets[idx];
        execution_rules::stop_reference_price(
            &slot.market_state,
            slot.contract_spec.mark_method(),
            price_type,
            side,
        )
    }

    fn evaluate_pending_stops_for_market(
        &mut self,
        idx: usize,
        candle_range: Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>,
    ) {
        self.elected_stops_scratch.clear();
        let slot = &mut self.markets[idx];
        let mark_method = slot.contract_spec.mark_method();
        let ms = &slot.market_state;
        slot.pending_stops.evaluate_and_drain(
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

        let n = self.elected_stops_scratch.len();
        for i in 0..n {
            let (stop, trigger_price) = self.elected_stops_scratch[i];
            let intrabar =
                execution_rules::elected_intrabar(stop.trigger_price_type(), candle_range);
            self.execute_elected_stop_for_market(idx, stop, trigger_price, intrabar);
            if self.liquidated_during_fills {
                core::hint::cold_path();
                // The remaining elected stops were already removed from `pending_stops`,
                // so the liquidation's own stop cancellation does not see them. Report
                // them here, otherwise the user never learns their fate.
                let market_id = self.markets[idx].market_id;
                for j in (i + 1)..n {
                    let (left, _) = self.elected_stops_scratch[j];
                    self.push_stop_event(
                        market_id,
                        left.cancelled_event(StopCancelReason::ForcedCancel),
                    );
                }
                break;
            }
        }
        self.elected_stops_scratch.clear();
    }

    fn execute_elected_stop_for_market(
        &mut self,
        idx: usize,
        stop: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        trigger_price: QuoteCurrency<I, D>,
        from_candle: bool,
    ) {
        match stop.kind() {
            StopKind::StopMarket | StopKind::TakeProfitMarket | StopKind::TrailingStop { .. } => {
                self.execute_elected_market_stop_for_market(idx, stop, trigger_price, from_candle);
            }
            StopKind::StopLimit {
                limit_price,
                time_in_force,
            }
            | StopKind::TakeProfitLimit {
                limit_price,
                time_in_force,
            } => {
                self.execute_elected_limit_stop_for_market(
                    idx,
                    stop,
                    trigger_price,
                    limit_price,
                    time_in_force,
                );
            }
        }
    }

    fn execute_elected_market_stop_for_market(
        &mut self,
        idx: usize,
        stop: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        trigger_price: QuoteCurrency<I, D>,
        from_candle: bool,
    ) {
        let market_id = self.markets[idx].market_id;
        let exec_qty = if stop.reduce_only() {
            let Some(clamped) =
                self.markets[idx].clamp_reduce_only_quantity(stop.side(), stop.quantity())
            else {
                self.push_stop_event(
                    market_id,
                    stop.cancelled_event(StopCancelReason::ReduceOnlyNoPosition),
                );
                return;
            };
            clamped
        } else {
            stop.quantity()
        };

        let slot = &self.markets[idx];
        let fill_price = execution_rules::market_stop_fill_price(
            slot.contract_spec.slippage_model(),
            slot.contract_spec.price_filter().tick_size(),
            &slot.market_state,
            stop.side(),
            exec_qty,
            stop.stop_price(),
            from_candle,
        );

        let protection = stop
            .protection()
            .or_else(|| self.markets[idx].contract_spec.stop_protection());
        if let Some(collar) = protection
            && !collar.allows_execution(stop.side(), stop.stop_price(), fill_price)
        {
            self.push_stop_event(
                market_id,
                stop.cancelled_event(StopCancelReason::StopProtectionBand),
            );
            return;
        }

        if self
            .check_portfolio_market_order(idx, stop.side(), exec_qty, fill_price)
            .is_err()
        {
            self.push_stop_event(
                market_id,
                stop.cancelled_event(StopCancelReason::InsufficientMargin),
            );
            return;
        }

        let notional = BaseOrQuote::PairedCurrency::convert_from(exec_qty, fill_price);
        let fee = notional * *self.markets[idx].contract_spec.fee_taker().as_ref();
        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        let realized = self.book_fill(idx, exec_qty, fill_price, stop.side(), fee);
        self.record_ledger_flows(
            ts_ns,
            Some(market_id),
            Some(stop.id()),
            fill_flows(realized, fee),
            eq_before,
        );
        let solvency = self.reconcile_portfolio_margin(bad_debt_before, ts_ns);
        self.drain_forced_cancels_into_events();

        self.push_stop_event(
            market_id,
            StopOrderEvent::Filled {
                stop_order_id: stop.id(),
                user_order_id: stop.user_order_id(),
                side: stop.side(),
                filled_quantity: exec_qty,
                trigger_price,
                fill_price,
                fee,
                solvency,
            },
        );
        if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
            self.liquidated_during_fills = true;
        }
    }

    fn execute_elected_limit_stop_for_market(
        &mut self,
        idx: usize,
        stop: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        trigger_price: QuoteCurrency<I, D>,
        limit_price: QuoteCurrency<I, D>,
        time_in_force: TimeInForce,
    ) {
        let market_id = self.markets[idx].market_id;
        let Ok(child) = LimitOrder::new_with_user_order_id(
            stop.side(),
            limit_price,
            stop.quantity(),
            stop.user_order_id(),
        ) else {
            self.push_stop_event(
                market_id,
                stop.cancelled_event(StopCancelReason::ChildLimitRejected),
            );
            return;
        };
        let child = child
            .with_time_in_force(time_in_force)
            .with_reduce_only(stop.reduce_only());
        assert!(self.forced_cancel_scratch.is_empty());
        let submission = self.submit_limit_order_keep_cancels(market_id, child);
        // Orders the venue cancelled while settling an immediate child fill must reach the
        // event stream (with their own market tags) like any other forced cancel.
        self.drain_forced_cancels_into_events();
        match submission {
            Ok(crate::exchange::LimitOrderSubmission::Resting(pending)) => {
                self.push_stop_event(
                    market_id,
                    StopOrderEvent::PlacedLimit {
                        stop_order_id: stop.id(),
                        user_order_id: stop.user_order_id(),
                        child_order_id: pending.id(),
                        trigger_price,
                    },
                );
            }
            Ok(crate::exchange::LimitOrderSubmission::ImmediatelyFilled {
                filled_order,
                fee,
                forced_cancels,
                solvency,
            }) => {
                assert!(forced_cancels.is_empty(), "routed via the scratch buffer");
                self.push_stop_event(
                    market_id,
                    StopOrderEvent::Filled {
                        stop_order_id: stop.id(),
                        user_order_id: stop.user_order_id(),
                        side: stop.side(),
                        filled_quantity: filled_order.total_quantity(),
                        trigger_price,
                        fill_price: filled_order.state().avg_fill_price(),
                        fee,
                        solvency,
                    },
                );
                if matches!(solvency, Solvency::Liquidated | Solvency::Bankrupt) {
                    self.liquidated_during_fills = true;
                }
            }
            Err(PortfolioError::SubmitLimitOrder(SubmitLimitOrderError::ReduceOnlyRejected)) => {
                self.push_stop_event(
                    market_id,
                    stop.cancelled_event(StopCancelReason::ReduceOnlyNoPosition),
                );
            }
            Err(_) => {
                self.push_stop_event(
                    market_id,
                    stop.cancelled_event(StopCancelReason::ChildLimitRejected),
                );
            }
        }
    }

    /// Clear `stop_events` at the start of a public call which can emit stop events.
    ///
    /// Also restores the capacity bound (one event per possible pending stop of every
    /// market) if it is missing. `Clone` and `add_market` keep it, so only a deserialized
    /// portfolio (serde cannot restore spare `Vec` capacity) takes that branch.
    #[inline]
    fn reset_stop_events(&mut self) {
        self.stop_events.clear();
        if self.stop_events.capacity() < self.max_pending_stops {
            core::hint::cold_path();
            self.stop_events.reserve_exact(self.max_pending_stops);
        }
        assert!(self.stop_events.capacity() >= self.max_pending_stops);
        assert!(self.stop_events.is_empty());
    }

    /// Record a stop event. Cannot overflow: see the `stop_events` field docs.
    #[inline]
    fn push_stop_event(
        &mut self,
        market_id: MarketId,
        event: StopOrderEvent<I, D, BaseOrQuote, UserOrderIdT>,
    ) {
        assert2::debug_assert!(self.stop_events.len() < self.max_pending_stops);
        self.stop_events
            .push_within_capacity(PortfolioStopEvent { market_id, event })
            .expect("each pending stop yields at most one event per call");
    }
}
