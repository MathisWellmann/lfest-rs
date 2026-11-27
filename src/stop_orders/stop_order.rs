//! Conditional / stop order data types, trigger rules, and lifecycle events.

use const_decimal::Decimal;
use getset::CopyGetters;
use num_traits::{
    One,
    Zero,
};
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use super::StopProtection;
use crate::{
    order_rate_limiter::RateLimitReached,
    types::{
        Currency,
        MarginCurrency,
        Mon,
        OrderId,
        QuoteCurrency,
        Side,
        Solvency,
        TimeInForce,
        TimestampNs,
        UserOrderId,
        ValidateOrderQuantityError,
    },
};

/// Which market price feeds a stop order's trigger and watermark evaluation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TriggerPriceType {
    /// Contract mark price (from `ContractSpecification::mark_method()`).
    #[default]
    Mark,
    /// Last traded price (falls back to mid price when no trade has printed yet).
    Last,
    /// Best opposite top-of-book quote (`ask` for buy stops, `bid` for sell stops).
    BestBidAsk,
}

/// Direction in which the reference price must cross `stop_price` to elect the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TriggerDirection {
    /// Triggers when `reference_price >= stop_price` (e.g. buy stop-loss, sell take-profit).
    AtOrAbove,
    /// Triggers when `reference_price <= stop_price` (e.g. sell stop-loss, buy take-profit).
    AtOrBelow,
}

/// Trail distance for [`StopKind::TrailingStop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TrailDistance<I, const D: u8>
where
    I: Mon<D>,
{
    /// Fixed absolute offset in quote price units.
    Absolute(QuoteCurrency<I, D>),
    /// Proportional callback fraction in `(0, 1)` (e.g. `0.05` = 5% trailing stop).
    CallbackRate(Decimal<I, D>),
}

/// Concrete conditional order behavior once elected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StopKind<I, const D: u8>
where
    I: Mon<D>,
{
    /// Triggers when price moves adversely (`Buy`: `>= stop`, `Sell`: `<= stop`)
    /// and executes immediately as a market order (optionally bounded by [`StopProtection`]).
    StopMarket,
    /// Triggers like `StopMarket`, then submits a limit order at `limit_price` with `tif`.
    StopLimit {
        /// Limit price of the child limit order submitted upon election.
        limit_price: QuoteCurrency<I, D>,
        /// Time-in-force of the child limit order.
        time_in_force: TimeInForce,
    },
    /// Triggers when price moves favorably (`Buy`: `<= stop`, `Sell`: `>= stop`)
    /// and executes immediately as a market order.
    TakeProfitMarket,
    /// Triggers like `TakeProfitMarket`, then submits a limit order at `limit_price` with `tif`.
    TakeProfitLimit {
        /// Limit price of the child limit order submitted upon election.
        limit_price: QuoteCurrency<I, D>,
        /// Time-in-force of the child limit order.
        time_in_force: TimeInForce,
    },
    /// Off-book trailing stop whose watermark follows favorable prints and whose
    /// stop price only ever tightens (adapted from `OrderBook-rs`).
    TrailingStop {
        /// Fixed price offset or percentage callback rate.
        trail: TrailDistance<I, D>,
        /// Favorable extreme reference price observed since admission.
        watermark: QuoteCurrency<I, D>,
    },
}

/// A pending off-book conditional / stop order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CopyGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StopOrder<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Venue-assigned order id (assigned on submission; `0` prior to admission).
    #[getset(get_copy = "pub")]
    id: OrderId,

    /// User-assigned order id.
    #[getset(get_copy = "pub")]
    user_order_id: UserOrderIdT,

    /// Order execution side once triggered.
    #[getset(get_copy = "pub")]
    side: Side,

    /// Order quantity in contract size currency.
    #[getset(get_copy = "pub")]
    quantity: BaseOrQuote,

    /// Current trigger / stop price (trails over time for `TrailingStop`).
    #[getset(get_copy = "pub")]
    stop_price: QuoteCurrency<I, D>,

    /// Conditional order kind and child execution parameters.
    #[getset(get_copy = "pub")]
    kind: StopKind<I, D>,

    /// Which price feed drives election and trailing updates.
    #[getset(get_copy = "pub")]
    trigger_price_type: TriggerPriceType,

    /// Whether the triggered child order is strictly reduce-only.
    #[getset(get_copy = "pub")]
    reduce_only: bool,

    /// Optional protection collar capping execution slippage on stop-market triggers.
    #[getset(get_copy = "pub")]
    protection: Option<StopProtection<I, D>>,

    /// Exchange admission timestamp.
    #[getset(get_copy = "pub")]
    ts_submitted_ns: TimestampNs,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> StopOrder<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new `StopMarket` order.
    ///
    /// # Errors
    /// Returns [`StopOrderError`] if `stop_price <= 0` or `quantity <= 0`.
    pub fn stop_market(
        side: Side,
        stop_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, StopOrderError> {
        Self::build(
            side,
            stop_price,
            quantity,
            StopKind::StopMarket,
            user_order_id,
        )
    }

    /// Create a new `StopLimit` order.
    ///
    /// # Errors
    /// Returns [`StopOrderError`] if `stop_price <= 0`, `limit_price <= 0`, or `quantity <= 0`.
    pub fn stop_limit(
        side: Side,
        stop_price: QuoteCurrency<I, D>,
        limit_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        time_in_force: TimeInForce,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, StopOrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(StopOrderError::InvalidLimitPrice);
        }
        Self::build(
            side,
            stop_price,
            quantity,
            StopKind::StopLimit {
                limit_price,
                time_in_force,
            },
            user_order_id,
        )
    }

    /// Create a new `TakeProfitMarket` order.
    ///
    /// # Errors
    /// Returns [`StopOrderError`] if `stop_price <= 0` or `quantity <= 0`.
    pub fn take_profit_market(
        side: Side,
        stop_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, StopOrderError> {
        Self::build(
            side,
            stop_price,
            quantity,
            StopKind::TakeProfitMarket,
            user_order_id,
        )
    }

    /// Create a new `TakeProfitLimit` order.
    ///
    /// # Errors
    /// Returns [`StopOrderError`] if `stop_price <= 0`, `limit_price <= 0`, or `quantity <= 0`.
    pub fn take_profit_limit(
        side: Side,
        stop_price: QuoteCurrency<I, D>,
        limit_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        time_in_force: TimeInForce,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, StopOrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(StopOrderError::InvalidLimitPrice);
        }
        Self::build(
            side,
            stop_price,
            quantity,
            StopKind::TakeProfitLimit {
                limit_price,
                time_in_force,
            },
            user_order_id,
        )
    }

    /// Create a new `TrailingStop` order anchored at `initial_watermark`.
    ///
    /// Initial stop price is `initial_watermark - trail` for `Sell` or
    /// `initial_watermark + trail` for `Buy`.
    ///
    /// # Errors
    /// Returns [`StopOrderError`] if watermark, trail distance, resulting stop price, or
    /// quantity are invalid.
    pub fn trailing_stop(
        side: Side,
        initial_watermark: QuoteCurrency<I, D>,
        trail: TrailDistance<I, D>,
        quantity: BaseOrQuote,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, StopOrderError> {
        if initial_watermark <= QuoteCurrency::zero() {
            return Err(StopOrderError::InvalidStopPrice);
        }
        match trail {
            TrailDistance::Absolute(offset) => {
                if offset <= QuoteCurrency::zero() {
                    return Err(StopOrderError::InvalidTrailDistance);
                }
            }
            TrailDistance::CallbackRate(rate) => {
                if rate <= Decimal::zero() || rate >= Decimal::one() {
                    return Err(StopOrderError::InvalidTrailDistance);
                }
            }
        }
        let stop_price = Self::compute_trailed_stop(side, initial_watermark, trail);
        if stop_price <= QuoteCurrency::zero() {
            return Err(StopOrderError::InvalidStopPrice);
        }
        Self::build(
            side,
            stop_price,
            quantity,
            StopKind::TrailingStop {
                trail,
                watermark: initial_watermark,
            },
            user_order_id,
        )
    }

    fn build(
        side: Side,
        stop_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        kind: StopKind<I, D>,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, StopOrderError> {
        if stop_price <= QuoteCurrency::zero() {
            return Err(StopOrderError::InvalidStopPrice);
        }
        if quantity <= BaseOrQuote::zero() {
            return Err(StopOrderError::ValidateOrderQuantity(
                ValidateOrderQuantityError::OrderQuantityLTEZero,
            ));
        }
        Ok(Self {
            id: OrderId::default(),
            user_order_id,
            side,
            quantity,
            stop_price,
            kind,
            trigger_price_type: TriggerPriceType::default(),
            reduce_only: false,
            protection: None,
            ts_submitted_ns: TimestampNs::default(),
        })
    }

    /// Select the [`TriggerPriceType`] (default [`TriggerPriceType::Mark`]).
    #[inline]
    #[must_use]
    pub fn with_trigger_price_type(mut self, trigger_price_type: TriggerPriceType) -> Self {
        self.trigger_price_type = trigger_price_type;
        self
    }

    /// Mark the stop order as `reduce_only` (default `false`).
    #[inline]
    #[must_use]
    pub fn with_reduce_only(mut self, reduce_only: bool) -> Self {
        self.reduce_only = reduce_only;
        self
    }

    /// Attach a [`StopProtection`] collar capping slippage on market triggers.
    #[inline]
    #[must_use]
    pub fn with_protection(mut self, protection: StopProtection<I, D>) -> Self {
        self.protection = Some(protection);
        self
    }

    /// Assign venue metadata upon admission.
    #[inline]
    pub(crate) fn stamp_admitted(&mut self, id: OrderId, ts_ns: TimestampNs) {
        self.id = id;
        self.ts_submitted_ns = ts_ns;
    }

    /// Return the trigger crossing direction for this stop order.
    #[inline]
    #[must_use]
    pub fn trigger_direction(&self) -> TriggerDirection {
        match (&self.kind, self.side) {
            (
                StopKind::StopMarket | StopKind::StopLimit { .. } | StopKind::TrailingStop { .. },
                Side::Buy,
            )
            | (StopKind::TakeProfitMarket | StopKind::TakeProfitLimit { .. }, Side::Sell) => {
                TriggerDirection::AtOrAbove
            }
            (
                StopKind::StopMarket | StopKind::StopLimit { .. } | StopKind::TrailingStop { .. },
                Side::Sell,
            )
            | (StopKind::TakeProfitMarket | StopKind::TakeProfitLimit { .. }, Side::Buy) => {
                TriggerDirection::AtOrBelow
            }
        }
    }

    /// Return `true` if `price` crosses this stop order's trigger price.
    #[inline]
    #[must_use]
    pub fn is_triggered_by(&self, price: QuoteCurrency<I, D>) -> bool {
        match self.trigger_direction() {
            TriggerDirection::AtOrAbove => price >= self.stop_price,
            TriggerDirection::AtOrBelow => price <= self.stop_price,
        }
    }

    /// Trail the watermark and monotonically tighten `stop_price` when `self` is a
    /// [`StopKind::TrailingStop`]. Returns `true` if the stop price tightened.
    #[inline]
    pub fn trail_to(&mut self, favorable_price: QuoteCurrency<I, D>) -> bool {
        let StopKind::TrailingStop { trail, watermark } = &mut self.kind else {
            return false;
        };
        // A sell stop protects a long, so higher prices favor it; a buy stop the opposite.
        let side = self.side;
        let favors = |a: QuoteCurrency<I, D>, b: QuoteCurrency<I, D>| match side {
            Side::Sell => a > b,
            Side::Buy => a < b,
        };
        if !favors(favorable_price, *watermark) {
            return false;
        }
        *watermark = favorable_price;
        let candidate = Self::compute_trailed_stop(self.side, favorable_price, *trail);
        if candidate <= QuoteCurrency::zero() {
            return false;
        }
        let tightened = favors(candidate, self.stop_price);
        if tightened {
            self.stop_price = candidate;
        }
        tightened
    }

    #[inline]
    fn compute_trailed_stop(
        side: Side,
        watermark: QuoteCurrency<I, D>,
        trail: TrailDistance<I, D>,
    ) -> QuoteCurrency<I, D> {
        let offset = match trail {
            TrailDistance::Absolute(off) => off,
            TrailDistance::CallbackRate(rate) => watermark * rate,
        };
        match side {
            Side::Sell => watermark - offset,
            Side::Buy => watermark + offset,
        }
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> StopOrder<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The event reporting that this stop was cancelled for `reason`.
    #[inline]
    #[must_use]
    pub(crate) fn cancelled_event(
        &self,
        reason: StopCancelReason,
    ) -> StopOrderEvent<I, D, BaseOrQuote, UserOrderIdT> {
        StopOrderEvent::Cancelled {
            stop_order_id: self.id(),
            user_order_id: self.user_order_id(),
            reason,
        }
    }
}

/// Reason a triggered or pending stop order was cancelled without executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StopCancelReason {
    /// Triggered market execution price fell outside the configured [`StopProtection`] collar.
    StopProtectionBand,
    /// Triggered `reduce_only` stop found no opposite-side position to reduce.
    ReduceOnlyNoPosition,
    /// Triggered stop failed margin/risk checks.
    InsufficientMargin,
    /// Triggered stop-limit child could not be placed (filter, capacity, or TIF rejection).
    ChildLimitRejected,
    /// Force-cancelled because the account was liquidated or the market was delisted.
    ForcedCancel,
}

/// Event emitted when a pending stop order triggers during `update_state` / `update_market`.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StopOrderEvent<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Stop-market / take-profit-market / trailing-stop elected and executed immediately.
    Filled {
        /// Id of the elected stop order.
        stop_order_id: OrderId,
        /// User id of the elected stop order.
        user_order_id: UserOrderIdT,
        /// Execution side.
        side: Side,
        /// Actual executed quantity (may be clamped if `reduce_only`).
        filled_quantity: BaseOrQuote,
        /// Reference price that elected the stop.
        trigger_price: QuoteCurrency<I, D>,
        /// Execution fill price.
        fill_price: QuoteCurrency<I, D>,
        /// Taker fee charged.
        fee: BaseOrQuote::PairedCurrency,
        /// Solvency after settling the stop fill.
        solvency: Solvency,
    },
    /// Stop-limit / take-profit-limit elected and placed a child limit order on the book
    /// (or immediately filled if marketable).
    PlacedLimit {
        /// Id of the elected stop order.
        stop_order_id: OrderId,
        /// User id of the elected stop order.
        user_order_id: UserOrderIdT,
        /// Venue id assigned to the child limit order.
        child_order_id: OrderId,
        /// Reference price that elected the stop.
        trigger_price: QuoteCurrency<I, D>,
    },
    /// Stop was elected or swept but cancelled for `reason`.
    Cancelled {
        /// Id of the stop order.
        stop_order_id: OrderId,
        /// User id of the stop order.
        user_order_id: UserOrderIdT,
        /// Reason the stop did not execute.
        reason: StopCancelReason,
    },
}

/// Errors returned when constructing, submitting, or cancelling a [`StopOrder`].
#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(missing_docs, reason = "Self documenting")]
pub enum StopOrderError {
    #[error("stop price must be strictly positive")]
    InvalidStopPrice,

    #[error("stop-limit child limit price must be strictly positive")]
    InvalidLimitPrice,

    #[error("trailing stop distance or callback rate is invalid")]
    InvalidTrailDistance,

    #[error("stop order would trigger immediately against current market price")]
    StopWouldTrigger,

    #[error("maximum number of pending stop orders ({0}) reached")]
    MaxPendingStops(u16),

    #[error("stop order id not found")]
    NotFound,

    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error(transparent)]
    ValidateOrderQuantity(#[from] ValidateOrderQuantityError),
}
