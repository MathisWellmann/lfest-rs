//! Core multi-symbol cross-margin [`PortfolioExchange`] implementation.

use std::collections::HashMap;

use const_decimal::Decimal;
use getset::Getters;
use num_traits::Zero;
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
        RePricing,
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
};

/// Internal per-market slot stored contiguously in [`PortfolioExchange::markets`].
#[derive(Debug, Clone)]
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
    fn has_positive_quotes(&self) -> bool {
        self.market_state.bid() > QuoteCurrency::zero()
            && self.market_state.ask() > QuoteCurrency::zero()
    }

    #[inline]
    fn unrealized_pnl(&self) -> BaseOrQuote::PairedCurrency {
        if self.position.quantity().is_zero() || !self.has_positive_quotes() {
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
        let maker_fee = (*self.contract_spec.fee_maker().as_ref()).max(Decimal::zero());
        (self.active_limit_orders.bids().notional_sum()
            + self.active_limit_orders.asks().notional_sum())
            * maker_fee
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
        let notional = if self.has_positive_quotes() {
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
        let pos_qty = self.position.quantity();
        let opposite_size = match side {
            Buy if pos_qty.is_negative() => pos_qty.abs(),
            Sell if pos_qty.is_positive() => pos_qty,
            _ => return None,
        };
        Some(crate::utils::min(quantity, opposite_size))
    }

    #[inline]
    fn largest_collateral_contributor(&self) -> Option<(OrderId, BaseOrQuote::PairedCurrency)> {
        let bids_notional = self.active_limit_orders.bids().notional_sum();
        let asks_notional = self.active_limit_orders.asks().notional_sum();
        let im = self.contract_spec.init_margin_req();
        let fee = (*self.contract_spec.fee_maker().as_ref()).max(Decimal::zero());
        let current_margin = self.order_margin();

        self.active_limit_orders
            .iter()
            .map(|order| {
                let (bids, asks) = match order.side() {
                    Buy => (bids_notional - order.notional(), asks_notional),
                    Sell => (bids_notional, asks_notional - order.notional()),
                };
                let margin_without = crate::utils::order_margin(bids, asks, im, &self.position);
                let delta = current_margin - margin_without + order.notional() * fee;
                (order.id(), delta)
            })
            .max_by_key(|&(_, delta)| delta)
    }
}

/// Multi-symbol cross-margin perpetual futures exchange.
///
/// All registered markets share a single [`Balances`] wallet. Profits in winning
/// positions offset losses in losing positions under cross-margin, and forced
/// cancellations / liquidations reconcile across the entire portfolio.
#[derive(Debug, Clone, Getters)]
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

    /// Conditional / stop order events emitted during the most recent [`Self::update_market`].
    #[getset(get = "pub")]
    stop_events: Vec<PortfolioStopEvent<I, D, BaseOrQuote, UserOrderIdT>>,

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
        assert!(
            n_markets <= usize::from(u16::MAX),
            "number of markets exceeds u16::MAX"
        );
        let balances = Balances::new(config.starting_wallet_balance());
        let mut markets = Vec::with_capacity(n_markets);
        let mut ticker_index = HashMap::with_capacity(n_markets);
        let mut total_order_cap: usize = 0;

        for (idx, market_cfg) in config.markets().iter().enumerate() {
            let market_id =
                MarketId::new(u16::try_from(idx).expect("n_markets was asserted <= u16::MAX"));
            total_order_cap += usize::from(market_cfg.max_num_open_orders().get());
            let ticker = market_cfg.contract_spec().ticker();
            if !ticker.is_empty() {
                ticker_index.insert(ticker.clone(), market_id);
            }
            markets
                .push_within_capacity(MarketSlot::new(market_id, market_cfg.clone()))
                .expect(EXPECT_CAPACITY);
        }

        let scratch_cap = (total_order_cap * 2).max(16);
        Self {
            balances,
            markets,
            ticker_index,
            next_order_id: OrderId::default(),
            order_events: Vec::with_capacity(scratch_cap * 2),
            stop_events: Vec::with_capacity(scratch_cap),
            elected_stops_scratch: Vec::with_capacity(scratch_cap),
            forced_cancel_scratch: Vec::with_capacity(scratch_cap),
            liquidated_during_fills: false,
            ledger: None,
        }
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

    #[inline]
    fn record_ledger_delta(
        &mut self,
        ts_ns: TimestampNs,
        market_id: Option<MarketId>,
        order_id: Option<OrderId>,
        kind: LedgerEntryKind,
        equity_before: BaseOrQuote::PairedCurrency,
        bad_debt_before: BaseOrQuote::PairedCurrency,
    ) {
        let Some(ledger) = self.ledger.as_mut() else {
            return;
        };
        let equity_after = self.balances.equity();
        let bad_debt_after = self.balances.bad_debt();
        let delta = equity_after - equity_before;
        if !delta.is_zero() {
            ledger.record(ts_ns, market_id, order_id, kind, delta, equity_after);
        }
        if bad_debt_after > bad_debt_before {
            ledger.record(
                ts_ns,
                market_id,
                order_id,
                LedgerEntryKind::BadDebtAbsorption,
                Zero::zero(),
                equity_after,
            );
        }
    }

    /// Dynamically register a new market and return its assigned [`MarketId`].
    ///
    /// # Errors
    /// Returns [`PortfolioError::MaxMarketsReached`] if `u16::MAX` markets are already registered.
    pub fn add_market(
        &mut self,
        market_cfg: MarketConfig<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<MarketId, PortfolioError<UserOrderIdT>> {
        let Ok(raw_id) = u16::try_from(self.markets.len()) else {
            return Err(PortfolioError::MaxMarketsReached(usize::from(u16::MAX)));
        };
        let market_id = MarketId::new(raw_id);
        let ticker = market_cfg.contract_spec().ticker();
        if !ticker.is_empty() {
            self.ticker_index.insert(ticker.clone(), market_id);
        }
        let add_cap = usize::from(market_cfg.max_num_open_orders().get()) * 2;
        self.forced_cancel_scratch.reserve(add_cap);
        self.order_events.reserve(add_cap * 2);
        self.stop_events.reserve(add_cap);
        self.elected_stops_scratch.reserve(add_cap);
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
    /// To delist a market and settle any open position, call [`Self::delist_market`].
    ///
    /// # Errors
    /// Returns [`PortfolioError::UnknownMarket`] or [`PortfolioError::MarketDelisted`] if already delisted.
    pub fn set_market_status(
        &mut self,
        market_id: MarketId,
        status: MarketStatus,
    ) -> Result<(), PortfolioError<UserOrderIdT>> {
        let slot = self.slot_mut(market_id)?;
        if slot.status == MarketStatus::Delisted {
            return Err(PortfolioError::MarketDelisted(market_id));
        }
        slot.status = status;
        Ok(())
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

        let cancelled_orders = self.markets[idx].active_limit_orders.cancel_all(None);
        if !self.markets[idx].pending_stops.is_empty() {
            let mut drained = Vec::with_capacity(self.markets[idx].pending_stops.len());
            self.markets[idx].pending_stops.drain_all(&mut drained);
            for stop in drained {
                self.push_stop_event(
                    market_id,
                    StopOrderEvent::Cancelled {
                        stop_order_id: stop.id(),
                        user_order_id: stop.user_order_id(),
                        reason: StopCancelReason::ForcedCancel,
                    },
                );
            }
        }
        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        let settled_quantity = self.markets[idx].position.quantity();
        let realized_pnl = if !settled_quantity.is_zero() {
            let pnl = BaseOrQuote::PairedCurrency::pnl(
                self.markets[idx].position.entry_price(),
                settlement_price,
                settled_quantity,
            );
            let close_side = if settled_quantity.is_positive() {
                Sell
            } else {
                Buy
            };
            self.markets[idx].position.change(
                settled_quantity.abs(),
                settlement_price,
                close_side,
                &mut self.balances,
            );
            self.record_ledger_delta(
                ts_ns,
                Some(market_id),
                None,
                LedgerEntryKind::DelistSettlement,
                eq_before,
                bad_debt_before,
            );
            pnl
        } else {
            Zero::zero()
        };

        self.markets[idx].status = MarketStatus::Delisted;
        let solvency = self.reconcile_portfolio_margin(bad_debt_before);
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
    /// Returns [`RiskError::Liquidate`] if the portfolio fell below maintenance margin
    /// (all positions were force-liquidated and resting orders cancelled, recorded in
    /// [`Self::order_events`]).
    ///
    /// # Panics
    /// Panics if `market_id` is out of range.
    pub fn update_market<U>(
        &mut self,
        market_id: MarketId,
        market_update: &U,
    ) -> Result<&Vec<PortfolioOrderEvent<I, D, BaseOrQuote, UserOrderIdT>>, RiskError>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
    {
        self.order_events.clear();
        self.stop_events.clear();
        self.liquidated_during_fills = false;

        let idx = market_id.as_usize();
        assert!(idx < self.markets.len(), "unknown market id {market_id}");

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
                return Err(RiskError::Liquidate);
            }
        }

        if self.has_any_open_position() && self.total_equity() < self.total_maintenance_margin() {
            core::hint::cold_path();
            self.force_liquidate_portfolio();
            self.drain_forced_cancels_into_events();
            return Err(RiskError::Liquidate);
        }

        if U::CAN_FILL_LIMIT_ORDERS && self.markets[idx].status != MarketStatus::Delisted {
            self.check_active_orders_for_market(idx, market_update.clone());
            if self.liquidated_during_fills {
                core::hint::cold_path();
                return Err(RiskError::Liquidate);
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
        assert!(
            mark_price > QuoteCurrency::zero(),
            "funding mark price must be strictly positive, got {mark_price}"
        );
        let idx = self.validate_market_index(market_id)?;
        let qty = self.markets[idx].position.quantity();
        let cashflow = if qty.is_zero() || rate.as_ref().is_zero() {
            Zero::zero()
        } else {
            let signed_val = BaseOrQuote::PairedCurrency::convert_from(qty, mark_price);
            -(signed_val * *rate.as_ref())
        };

        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        if !cashflow.is_zero() {
            self.balances.apply_funding(cashflow);
            self.record_ledger_delta(
                ts_ns,
                Some(market_id),
                None,
                LedgerEntryKind::Funding,
                eq_before,
                bad_debt_before,
            );
        }
        let solvency = self.reconcile_portfolio_margin(bad_debt_before);
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
            )
            .execution_price();
        if !strictly_reducing {
            let notional = BaseOrQuote::PairedCurrency::convert_from(order.quantity(), fill_price);
            self.markets[idx]
                .contract_spec
                .notional_filter()
                .validate_order_notional(notional)
                .map_err(SubmitMarketOrderError::from)?;
        }

        self.check_portfolio_market_order(idx, order.side(), order.quantity(), fill_price)
            .map_err(SubmitMarketOrderError::from)?;

        let meta = ExchangeOrderMeta::new(self.next_order_id(), ts_ns);
        let filled_order = order.into_pending(meta).into_filled(fill_price, ts_ns);

        let notional =
            BaseOrQuote::PairedCurrency::convert_from(filled_order.quantity(), fill_price);
        let fee = notional * *self.markets[idx].contract_spec.fee_taker().as_ref();

        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        self.markets[idx].position.change(
            filled_order.quantity(),
            fill_price,
            filled_order.side(),
            &mut self.balances,
        );
        self.balances.account_for_fee(fee);
        self.record_ledger_delta(
            ts_ns,
            Some(market_id),
            Some(filled_order.state().meta().id()),
            LedgerEntryKind::RealizedPnl,
            eq_before,
            bad_debt_before,
        );

        let solvency = self.reconcile_portfolio_margin(bad_debt_before);
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
    /// # Errors
    /// Returns [`PortfolioError`] on unknown/delisted/reduce-only market, rate limit,
    /// filter violation, crossing GTX order, capacity limit, or insufficient cross-margin.
    #[allow(
        clippy::too_many_lines,
        reason = "keeping all TIF and reduce-only admission branches in one place preserves clarity"
    )]
    pub fn submit_limit_order_with_execution(
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
            .order_rate_limiter
            .aquire(ts_ns)
            .map_err(SubmitLimitOrderError::from)?;
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
            let Some(clamped) = self.markets[idx]
                .clamp_reduce_only_quantity(order.side(), order.remaining_quantity())
            else {
                return Err(SubmitLimitOrderError::ReduceOnlyRejected.into());
            };
            if clamped < order.remaining_quantity() {
                order.set_remaining_quantity(clamped);
            }
        }

        let strictly_reducing =
            self.markets[idx].is_strictly_reducing(order.side(), order.remaining_quantity());
        if status == MarketStatus::ReduceOnly && !strictly_reducing {
            return Err(PortfolioError::MarketReduceOnly(market_id));
        }
        if !strictly_reducing {
            let notional = BaseOrQuote::PairedCurrency::convert_from(
                order.remaining_quantity(),
                order.limit_price(),
            );
            self.markets[idx]
                .contract_spec
                .notional_filter()
                .validate_order_notional(notional)
                .map_err(SubmitLimitOrderError::from)?;
        }

        let marketable = match order.side() {
            Buy => order.limit_price() >= self.markets[idx].market_state.ask(),
            Sell => order.limit_price() <= self.markets[idx].market_state.bid(),
        };
        let is_post_only = order.re_pricing() == RePricing::GoodTilCrossing
            && order.time_in_force() == TimeInForce::GoodTilCrossing;

        if marketable && is_post_only {
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

        if marketable {
            let fill_price = match order.side() {
                Buy => self.markets[idx].market_state.ask(),
                Sell => self.markets[idx].market_state.bid(),
            };
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
            self.markets[idx].position.change(
                filled_qty,
                fill_price,
                pending.side(),
                &mut self.balances,
            );
            self.balances.account_for_fee(fee);
            self.record_ledger_delta(
                ts_ns,
                Some(market_id),
                Some(pending.id()),
                LedgerEntryKind::RealizedPnl,
                eq_before,
                bad_debt_before,
            );
            let solvency = self.reconcile_portfolio_margin(bad_debt_before);
            let tagged_cancels = self.take_forced_cancels();
            let mut forced_cancels = Vec::with_capacity(tagged_cancels.len());
            for (_, o) in tagged_cancels {
                forced_cancels
                    .push_within_capacity(o)
                    .expect(EXPECT_CAPACITY);
            }
            let filled_order = pending.into_filled_at_price(fill_price, ts_ns);
            return Ok(crate::exchange::LimitOrderSubmission::ImmediatelyFilled {
                filled_order,
                fee,
                forced_cancels,
                solvency,
            });
        }

        if matches!(
            order.time_in_force(),
            TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill
        ) {
            return Err(SubmitLimitOrderError::ImmediateExecutionNotMarketable.into());
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
    /// # Errors
    /// Returns [`PortfolioError`] on unknown/delisted/reduce-only market, rate limit,
    /// filter violation, crossing GTX order, capacity limit, or insufficient cross-margin.
    pub fn submit_limit_order(
        &mut self,
        market_id: MarketId,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    ) -> Result<PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>, PortfolioError<UserOrderIdT>>
    {
        match self.submit_limit_order_with_execution(market_id, order)? {
            crate::exchange::LimitOrderSubmission::Resting(pending) => Ok(pending),
            crate::exchange::LimitOrderSubmission::ImmediatelyFilled { filled_order, .. } => {
                let meta = filled_order.state().meta().clone();
                let mut pending = LimitOrder::new_with_user_order_id(
                    filled_order.side(),
                    filled_order.limit_price(),
                    filled_order.total_quantity(),
                    filled_order.user_order_id(),
                )
                .expect("filled order had positive price and quantity")
                .with_re_pricing(filled_order.re_pricing())
                .with_time_in_force(filled_order.time_in_force())
                .with_reduce_only(filled_order.reduce_only())
                .into_pending(meta);
                pending.fill(filled_order.total_quantity());
                Ok(pending)
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
                if existing_order_id < self.next_order_id {
                    AmendLimitOrderError::OrderNoLongerActive
                } else {
                    AmendLimitOrderError::OrderIdNotFound {
                        order_id: existing_order_id,
                    }
                }
            })?;

        let qty_delta = new_order.total_quantity() - existing.total_quantity();
        let new_leaves_qty = existing.remaining_quantity() + qty_delta;
        if new_leaves_qty <= BaseOrQuote::zero() {
            self.markets[idx]
                .active_limit_orders
                .remove_limit_order(CancelBy::OrderId(existing_order_id))
                .expect("existing active order can be cancelled");
            return Err(AmendLimitOrderError::AmendQtyAlreadyFilled.into());
        }
        new_order.set_remaining_quantity(new_leaves_qty);
        self.markets[idx]
            .active_limit_orders
            .remove_limit_order(CancelBy::OrderId(existing_order_id))
            .expect("existing active order can be cancelled");
        self.submit_limit_order(market_id, new_order)
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

        // Position flip: compute post-trade portfolio equity vs total initial margin.
        let flip_qty = quantity - abs_qty;
        let total_notional = BaseOrQuote::PairedCurrency::convert_from(quantity, fill_price);
        let flip_notional = BaseOrQuote::PairedCurrency::convert_from(flip_qty, fill_price);
        let fee = total_notional * *slot.contract_spec.fee_taker().as_ref();
        let realized_pnl =
            BaseOrQuote::PairedCurrency::pnl(slot.position.entry_price(), fill_price, pos_qty);

        // Remove old market's uPnL and position margin, add realized_pnl - fee and flip position margin.
        let other_upnl = self.total_unrealized_pnl() - slot.unrealized_pnl();
        let other_req = self.total_required_collateral() - slot.position_margin();
        let post_equity = self.balances.equity() + realized_pnl - fee + other_upnl;
        let post_req = other_req + flip_notional * slot.contract_spec.init_margin_req();
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
                        return;
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
                        return;
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

        self.markets[idx]
            .position
            .change(filled_quantity, limit_price, side, &mut self.balances);
        self.balances.account_for_fee(fee);
        self.record_ledger_delta(
            ts_ns,
            Some(market_id),
            Some(order.id()),
            LedgerEntryKind::RealizedPnl,
            eq_before,
            bad_debt_before,
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

        let solvency = self.reconcile_portfolio_margin(bad_debt_before);
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
    /// `fee_taker + liquidation_fee`.
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
            if !slot.pending_stops.is_empty() {
                let mut drained = Vec::with_capacity(slot.pending_stops.len());
                slot.pending_stops.drain_all(&mut drained);
                for stop in drained {
                    if self.stop_events.len() < self.stop_events.capacity() {
                        self.stop_events
                            .push_within_capacity(PortfolioStopEvent {
                                market_id,
                                event: StopOrderEvent::Cancelled {
                                    stop_order_id: stop.id(),
                                    user_order_id: stop.user_order_id(),
                                    reason: StopCancelReason::ForcedCancel,
                                },
                            })
                            .expect(EXPECT_CAPACITY);
                    }
                }
            }
        }
        let n_markets = self.markets.len();
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
            let fee_rate = *self.markets[idx].contract_spec.fee_taker().as_ref()
                + self.markets[idx].contract_spec.liquidation_fee();
            let fee = notional * fee_rate;
            let eq_before = self.balances.equity();
            let bd_before = self.balances.bad_debt();
            self.markets[idx]
                .position
                .change(qty, fill_price, side, &mut self.balances);
            self.balances.account_for_fee(fee);
            self.record_ledger_delta(
                ts_ns,
                Some(market_id),
                None,
                LedgerEntryKind::Liquidation,
                eq_before,
                bd_before,
            );
        }
        info!("balances after portfolio liquidation: {}", self.balances);
    }

    #[must_use]
    fn reconcile_portfolio_margin(
        &mut self,
        bad_debt_before: BaseOrQuote::PairedCurrency,
    ) -> Solvency {
        let liquidated = if self.has_any_open_position()
            && self.total_equity() < self.total_maintenance_margin()
        {
            core::hint::cold_path();
            self.force_liquidate_portfolio();
            true
        } else {
            false
        };

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
        let price = match price_type {
            TriggerPriceType::Mark => slot
                .market_state
                .mark_price(slot.contract_spec.mark_method()),
            TriggerPriceType::Last => {
                let last = slot.market_state.last_trade_price();
                if last > QuoteCurrency::zero() {
                    last
                } else {
                    slot.market_state.mid_price()
                }
            }
            TriggerPriceType::BestBidAsk => match side {
                Buy => slot.market_state.ask(),
                Sell => slot.market_state.bid(),
            },
        };
        if price > QuoteCurrency::zero() {
            Some(price)
        } else {
            None
        }
    }

    fn evaluate_pending_stops_for_market(
        &mut self,
        idx: usize,
        candle_range: Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>,
    ) {
        self.elected_stops_scratch.clear();
        let mark_method = self.markets[idx].contract_spec.mark_method();
        let ms = self.markets[idx].market_state.clone();
        self.markets[idx].pending_stops.evaluate_and_drain(
            |stop| {
                if let Some((low, high)) = candle_range {
                    return Some((low, high));
                }
                let p = match stop.trigger_price_type() {
                    TriggerPriceType::Mark => ms.mark_price(mark_method),
                    TriggerPriceType::Last => {
                        let last = ms.last_trade_price();
                        if last > QuoteCurrency::zero() {
                            last
                        } else {
                            ms.mid_price()
                        }
                    }
                    TriggerPriceType::BestBidAsk => match stop.side() {
                        Buy => ms.ask(),
                        Sell => ms.bid(),
                    },
                };
                if p > QuoteCurrency::zero() {
                    Some((p, p))
                } else {
                    None
                }
            },
            &mut self.elected_stops_scratch,
        );

        let n = self.elected_stops_scratch.len();
        for i in 0..n {
            let (stop, trigger_price) = self.elected_stops_scratch[i];
            self.execute_elected_stop_for_market(idx, stop, trigger_price, candle_range.is_some());
            if self.liquidated_during_fills {
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
                    StopOrderEvent::Cancelled {
                        stop_order_id: stop.id(),
                        user_order_id: stop.user_order_id(),
                        reason: StopCancelReason::ReduceOnlyNoPosition,
                    },
                );
                return;
            };
            clamped
        } else {
            stop.quantity()
        };

        let quote_fill = self.markets[idx]
            .contract_spec
            .slippage_model()
            .quote(&self.markets[idx].market_state, stop.side(), exec_qty)
            .execution_price();
        let fill_price = if from_candle {
            match stop.side() {
                Buy => crate::utils::max(stop.stop_price(), quote_fill),
                Sell => crate::utils::min(stop.stop_price(), quote_fill),
            }
        } else {
            quote_fill
        };

        let protection = stop
            .protection()
            .or_else(|| self.markets[idx].contract_spec.stop_protection());
        if let Some(collar) = protection
            && !collar.allows_execution(stop.side(), stop.stop_price(), fill_price)
        {
            self.push_stop_event(
                market_id,
                StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::StopProtectionBand,
                },
            );
            return;
        }

        if self
            .check_portfolio_market_order(idx, stop.side(), exec_qty, fill_price)
            .is_err()
        {
            self.push_stop_event(
                market_id,
                StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::InsufficientMargin,
                },
            );
            return;
        }

        let notional = BaseOrQuote::PairedCurrency::convert_from(exec_qty, fill_price);
        let fee = notional * *self.markets[idx].contract_spec.fee_taker().as_ref();
        let eq_before = self.balances.equity();
        let bad_debt_before = self.balances.bad_debt();
        let ts_ns = self.markets[idx].market_state.current_timestamp_ns();
        self.markets[idx]
            .position
            .change(exec_qty, fill_price, stop.side(), &mut self.balances);
        self.balances.account_for_fee(fee);
        self.record_ledger_delta(
            ts_ns,
            Some(market_id),
            Some(stop.id()),
            LedgerEntryKind::RealizedPnl,
            eq_before,
            bad_debt_before,
        );
        let solvency = self.reconcile_portfolio_margin(bad_debt_before);
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
                StopOrderEvent::Cancelled {
                    stop_order_id: stop.id(),
                    user_order_id: stop.user_order_id(),
                    reason: StopCancelReason::ChildLimitRejected,
                },
            );
            return;
        };
        let child = child
            .with_time_in_force(time_in_force)
            .with_reduce_only(stop.reduce_only());
        match self.submit_limit_order_with_execution(market_id, child) {
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
                solvency,
                ..
            }) => {
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
                    StopOrderEvent::Cancelled {
                        stop_order_id: stop.id(),
                        user_order_id: stop.user_order_id(),
                        reason: StopCancelReason::ReduceOnlyNoPosition,
                    },
                );
            }
            Err(_) => {
                self.push_stop_event(
                    market_id,
                    StopOrderEvent::Cancelled {
                        stop_order_id: stop.id(),
                        user_order_id: stop.user_order_id(),
                        reason: StopCancelReason::ChildLimitRejected,
                    },
                );
            }
        }
    }

    #[inline]
    fn push_stop_event(
        &mut self,
        market_id: MarketId,
        event: StopOrderEvent<I, D, BaseOrQuote, UserOrderIdT>,
    ) {
        if self.stop_events.len() < self.stop_events.capacity() {
            self.stop_events
                .push_within_capacity(PortfolioStopEvent { market_id, event })
                .expect(EXPECT_CAPACITY);
        }
    }
}
