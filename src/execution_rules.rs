//! Order admission and execution rules shared by [`crate::prelude::Exchange`] and
//! [`crate::prelude::PortfolioExchange`].
//!
//! Both venues implement the same order semantics on different account models. Keeping
//! the rules here as pure functions means a fix cannot land in only one of them.

use getset::CopyGetters;
use num_traits::Zero;

use crate::{
    contract_specification::MarkMethod,
    ledger::LedgerEntryKind,
    market_state::MarketState,
    order_filters::NotionalFilter,
    slippage::SlippageModel,
    stop_orders::TriggerPriceType,
    types::{
        Currency,
        LimitOrder,
        MarginCurrency,
        MarketId,
        MarketOrder,
        Mon,
        NewOrder,
        QuoteCurrency,
        Side,
        SubmitLimitOrderError,
        SubmitMarketOrderError,
        TimeInForce,
        UserOrderId,
        ValidateOrderNotionalError,
    },
};

/// The executable quantity of a `reduce_only` order of `(side, quantity)` against a
/// position of `position_qty`, or `None` if there is no opposite position to reduce.
#[inline]
#[must_use]
pub(crate) fn clamp_reduce_only_quantity<I, const D: u8, BaseOrQuote>(
    position_qty: BaseOrQuote,
    side: Side,
    quantity: BaseOrQuote,
) -> Option<BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    assert2::debug_assert!(quantity > BaseOrQuote::zero());
    let opposite_size = match side {
        Side::Buy if position_qty.is_negative() => position_qty.abs(),
        Side::Sell if position_qty.is_positive() => position_qty,
        _ => return None,
    };
    let clamped = crate::utils::min(quantity, opposite_size);
    assert2::debug_assert!(clamped > BaseOrQuote::zero());
    assert2::debug_assert!(clamped <= quantity);
    Some(clamped)
}

/// Clamp a new `reduce_only` market order to the open position of `position_qty`.
///
/// # Errors
/// [`SubmitMarketOrderError::ReduceOnlyRejected`] if there is no opposite position.
pub(crate) fn clamp_reduce_only_market_order<I, const D: u8, BaseOrQuote, UserOrderIdT>(
    order: &mut MarketOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    position_qty: BaseOrQuote,
) -> Result<(), SubmitMarketOrderError>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    assert!(order.reduce_only());
    let requested = order.quantity();
    let clamped = clamp_reduce_only_quantity(position_qty, order.side(), requested)
        .ok_or(SubmitMarketOrderError::ReduceOnlyRejected)?;
    order.set_quantity(clamped);
    assert!(order.quantity() <= position_qty.abs());
    assert!(order.quantity() <= requested);
    Ok(())
}

/// Clamp a new `reduce_only` limit order to the open position of `position_qty`.
///
/// # Errors
/// - [`SubmitLimitOrderError::ReduceOnlyRejected`] if there is no opposite position.
/// - [`SubmitLimitOrderError::FillOrKillNotFullyFillable`] if a `FillOrKill` order would
///   have to be clamped, because filling only the clamped part is a partial fill which
///   `FillOrKill` forbids.
pub(crate) fn clamp_reduce_only_limit_order<I, const D: u8, BaseOrQuote, UserOrderIdT>(
    order: &mut LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>,
    position_qty: BaseOrQuote,
) -> Result<(), SubmitLimitOrderError>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    assert!(order.reduce_only());
    let requested = order.remaining_quantity();
    let Some(clamped) = clamp_reduce_only_quantity(position_qty, order.side(), requested) else {
        return Err(SubmitLimitOrderError::ReduceOnlyRejected);
    };
    if clamped < requested {
        if order.time_in_force() == TimeInForce::FillOrKill {
            return Err(SubmitLimitOrderError::FillOrKillNotFullyFillable);
        }
        order.set_remaining_quantity(clamped);
    }
    assert!(order.remaining_quantity() <= position_qty.abs());
    assert!(order.remaining_quantity() <= requested);
    Ok(())
}

/// The fill price of a marketable (crossing) limit order of `(side, quantity)`.
///
/// The order takes liquidity like a market order, so it pays the contract's
/// `slippage_model` impact as well. Unlike a market order it must never fill beyond its
/// `limit_price`, so the impacted price is capped there.
#[inline]
#[must_use]
pub(crate) fn marketable_limit_fill_price<I, const D: u8, BaseOrQuote>(
    slippage_model: SlippageModel<I, D>,
    tick_size: QuoteCurrency<I, D>,
    market_state: &MarketState<I, D>,
    side: Side,
    quantity: BaseOrQuote,
    limit_price: QuoteCurrency<I, D>,
) -> QuoteCurrency<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    assert!(limit_price > QuoteCurrency::zero());
    let impacted = slippage_model
        .quote(market_state, side, quantity, tick_size)
        .execution_price();
    let fill_price = match side {
        Side::Buy => crate::utils::min(impacted, limit_price),
        Side::Sell => crate::utils::max(impacted, limit_price),
    };
    // A marketable order crosses the touch, so its fill lies between touch and limit.
    match side {
        Side::Buy => {
            assert2::debug_assert!(fill_price >= market_state.ask());
            assert!(fill_price <= limit_price);
        }
        Side::Sell => {
            assert2::debug_assert!(fill_price <= market_state.bid());
            assert!(fill_price >= limit_price);
        }
    }
    assert!(fill_price > QuoteCurrency::zero());
    fill_price
}

/// The fill price of an elected market-type stop of `(side, quantity)`.
///
/// The stop executes like a market order, paying the `slippage_model` impact. If a
/// candle's intrabar trades elected it (`intrabar`), the trigger happened at an unknown
/// time inside the bar while the quotes are those at its close, so it fills at the
/// worse of its `stop_price` and the impacted closing quote.
#[inline]
#[must_use]
pub(crate) fn market_stop_fill_price<I, const D: u8, BaseOrQuote>(
    slippage_model: SlippageModel<I, D>,
    tick_size: QuoteCurrency<I, D>,
    market_state: &MarketState<I, D>,
    side: Side,
    quantity: BaseOrQuote,
    stop_price: QuoteCurrency<I, D>,
    intrabar: bool,
) -> QuoteCurrency<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    assert!(stop_price > QuoteCurrency::zero());
    let quote_fill = slippage_model
        .quote(market_state, side, quantity, tick_size)
        .execution_price();
    if !intrabar {
        return quote_fill;
    }
    let fill_price = match side {
        Side::Buy => crate::utils::max(stop_price, quote_fill),
        Side::Sell => crate::utils::min(stop_price, quote_fill),
    };
    match side {
        Side::Buy => assert!(fill_price >= stop_price),
        Side::Sell => assert!(fill_price <= stop_price),
    }
    fill_price
}

/// The ledger cashflows of a fill which realized `realized` and charged `fee`
/// (negative: rebate): the realized pnl and the fee as separate entries, so the
/// journal shows what the trade made and what it cost.
#[inline]
#[must_use]
pub(crate) fn fill_flows<I, const D: u8, Margin>(
    realized: Margin,
    fee: Margin,
) -> [(LedgerEntryKind, Margin); 2]
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    [
        (LedgerEntryKind::RealizedPnl, realized),
        (LedgerEntryKind::TradingFee, -fee),
    ]
}

/// Validate the notional of an order of `quantity` executing (or resting) at `price`.
///
/// A `reduce_only` order skips `min_notional`, so a dust position below it can always be
/// closed; it cannot add risk because it is clamped to the position. `max_notional`
/// always applies: a plain order which happens to reduce the position is not exempt,
/// and no order may exceed the venue's size cap.
///
/// # Errors
/// [`ValidateOrderNotionalError`] if the notional is outside the applicable bounds.
#[inline]
pub(crate) fn validate_order_notional<I, const D: u8, BaseOrQuote>(
    filter: &NotionalFilter<I, D, BaseOrQuote::PairedCurrency>,
    quantity: BaseOrQuote,
    price: QuoteCurrency<I, D>,
    reduce_only: bool,
) -> Result<(), ValidateOrderNotionalError>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    assert2::debug_assert!(quantity > BaseOrQuote::zero());
    assert2::debug_assert!(price > QuoteCurrency::zero());
    let notional = BaseOrQuote::PairedCurrency::convert_from(quantity, price);
    match filter.validate_order_notional(notional) {
        // `NotionalFilter::new` ensures `min <= max`, so an order below the minimum is
        // also within the maximum.
        Err(ValidateOrderNotionalError::NotionalTooLow) if reduce_only => Ok(()),
        result => result,
    }
}

/// Cap the liquidation clearance fee at the equity left once the liquidation's realized
/// PnL and taker fee are booked.
///
/// The clearance fee pays the venue for closing the position; it is collected only from
/// what is left of the account. Charging more would make the fee alone create bad debt
/// which the venue would then absorb itself, i.e. pay its own fee.
#[inline]
#[must_use]
pub(crate) fn capped_clearance_fee<I, const D: u8, Margin>(
    clearance_fee: Margin,
    equity_left: Margin,
) -> Margin
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    assert!(clearance_fee >= Margin::zero());
    let capped = crate::utils::max(
        crate::utils::min(clearance_fee, equity_left),
        Margin::zero(),
    );
    assert!(capped >= Margin::zero());
    assert!(capped <= clearance_fee);
    if equity_left >= Margin::zero() {
        assert!(capped <= equity_left);
    }
    capped
}

/// The current point reference price of a stop of `(price_type, side)`, or `None` if the
/// market has not provided that price yet.
#[inline]
#[must_use]
pub(crate) fn stop_reference_price<I, const D: u8>(
    market_state: &MarketState<I, D>,
    mark_method: MarkMethod,
    price_type: TriggerPriceType,
    side: Side,
) -> Option<QuoteCurrency<I, D>>
where
    I: Mon<D>,
{
    let price = match price_type {
        TriggerPriceType::Mark => market_state.mark_price(mark_method),
        TriggerPriceType::Last => {
            let last = market_state.last_trade_price();
            if last > QuoteCurrency::zero() {
                last
            } else {
                market_state.mid_price()
            }
        }
        TriggerPriceType::BestBidAsk => match side {
            Side::Buy => market_state.ask(),
            Side::Sell => market_state.bid(),
        },
    };
    (price > QuoteCurrency::zero()).then_some(price)
}

/// The `(low, high)` band of reference prices a stop of `(price_type, side)` observed
/// during the current market update, or `None` if its reference price is unknown.
///
/// `trade_range` is the intrabar trade range of a candle update. Only a
/// [`TriggerPriceType::Last`] stop watches trades, so only it is tested against the
/// whole range. A `Mark` stop watches the mark price, which none of the [`MarkMethod`]s
/// derives from trades (a trade wick must not elect it, which is the point of fair-price
/// marking), and a `BestBidAsk` stop watches the quotes. Both use the current point
/// value of their feed.
#[inline]
#[must_use]
pub(crate) fn stop_trigger_band<I, const D: u8>(
    market_state: &MarketState<I, D>,
    mark_method: MarkMethod,
    price_type: TriggerPriceType,
    side: Side,
    trade_range: Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>,
) -> Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>
where
    I: Mon<D>,
{
    if price_type == TriggerPriceType::Last
        && let Some((low, high)) = trade_range
    {
        assert!(low <= high, "candle range must be ordered");
        assert!(low > QuoteCurrency::zero(), "candle low must be positive");
        return Some((low, high));
    }
    let p = stop_reference_price(market_state, mark_method, price_type, side)?;
    assert!(p > QuoteCurrency::zero());
    Some((p, p))
}

/// Whether a stop of `price_type` elected during an update with `trade_range` was elected
/// by an intrabar trade (so it fills at the worse of its stop price and the closing quote)
/// rather than by a point price.
#[inline]
#[must_use]
pub(crate) fn elected_intrabar<I, const D: u8>(
    price_type: TriggerPriceType,
    trade_range: Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>,
) -> bool
where
    I: Mon<D>,
{
    trade_range.is_some() && price_type == TriggerPriceType::Last
}

/// Capacity of the per-call event and scratch buffers of both exchanges.
///
/// The bounds depend only on the number of order slots `n`: the `max_num_open_orders`
/// of an [`crate::prelude::Exchange`], or the sum over all markets of a
/// [`crate::prelude::PortfolioExchange`]. Bids and asks each rest up to that many
/// orders, and the pending stops are capped at the same number.
/// `new`, `add_market` and `clone` of both exchanges size their buffers from here, so the
/// bounds cannot drift apart. The push paths use `push_within_capacity`, so a bound which
/// is too small panics instead of allocating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CopyGetters)]
pub(crate) struct BufferCapacities {
    /// Fills and forced cancellations of one call: up to `2n` fills of the resting bids and
    /// asks plus as many forced cancellations.
    #[getset(get_copy = "pub(crate)")]
    order_events: usize,
    /// Orders force-cancelled between two drains: each of the at most `2n` resting
    /// orders is cancelled at most once.
    #[getset(get_copy = "pub(crate)")]
    forced_cancels: usize,
    /// Stops elected by one evaluation, which covers a single market's pending stops.
    #[getset(get_copy = "pub(crate)")]
    elected_stops: usize,
    /// One event per pending stop per call; a portfolio liquidation drains every market.
    #[getset(get_copy = "pub(crate)")]
    stop_events: usize,
}

impl BufferCapacities {
    /// The capacities for `order_slots` slots, see the field docs for each bound.
    #[must_use]
    pub(crate) fn for_order_slots(order_slots: usize) -> Self {
        // Both callers sum `NonZeroU16` limits over at most `MarketId::COUNT` markets.
        assert!(order_slots <= MarketId::COUNT * usize::from(u16::MAX));
        let caps = Self {
            order_events: order_slots * 4,
            forced_cancels: order_slots * 2,
            elected_stops: order_slots,
            stop_events: order_slots,
        };
        assert!(caps.order_events >= caps.forced_cancels);
        assert!(caps.forced_cancels >= caps.elected_stops);
        caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{
        BaseCurrency,
        Bba,
        PriceFilter,
    };

    #[test]
    fn reduce_only_orders_skip_only_the_min_notional() {
        use ValidateOrderNotionalError::*;
        let filter = NotionalFilter::new(
            Some(QuoteCurrency::<i64, 5>::new(50, 0)),
            Some(QuoteCurrency::new(500, 0)),
        )
        .unwrap();
        // `(quantity in tenths at a price of 100, plain result, reduce-only result)`
        for (qty_tenths, plain, reduce_only) in [
            (4, Err(NotionalTooLow), Ok(())),
            (5, Ok(()), Ok(())),
            (50, Ok(()), Ok(())),
            (51, Err(NotionalTooHigh), Err(NotionalTooHigh)),
        ] {
            let validate = |reduce_only| {
                validate_order_notional(
                    &filter,
                    BaseCurrency::<i64, 5>::new(qty_tenths, 1),
                    QuoteCurrency::new(100, 0),
                    reduce_only,
                )
            };
            assert_eq!(validate(false), plain, "{qty_tenths}/10");
            assert_eq!(validate(true), reduce_only, "{qty_tenths}/10");
        }
    }

    #[test]
    fn buffer_capacities_scale_with_order_slots() {
        let caps = BufferCapacities::for_order_slots(10);
        assert_eq!(caps.order_events(), 40);
        assert_eq!(caps.forced_cancels(), 20);
        assert_eq!(caps.elected_stops(), 10);
        assert_eq!(caps.stop_events(), 10);

        // A portfolio without markets has no order slots and must not allocate.
        let empty = BufferCapacities::for_order_slots(0);
        assert_eq!(empty.order_events(), 0);
        assert_eq!(empty.forced_cancels(), 0);
        assert_eq!(empty.elected_stops(), 0);
        assert_eq!(empty.stop_events(), 0);

        // The largest valid input: every market id with the largest order cap.
        let max_slots = MarketId::COUNT * usize::from(u16::MAX);
        assert_eq!(
            BufferCapacities::for_order_slots(max_slots).order_events(),
            max_slots * 4
        );
    }

    #[test]
    #[should_panic]
    fn buffer_capacities_reject_more_slots_than_any_config_allows() {
        let _ = BufferCapacities::for_order_slots(MarketId::COUNT * usize::from(u16::MAX) + 1);
    }

    fn quoted_market(bid: i64, ask: i64) -> MarketState<i64, 5> {
        let mut ms = MarketState::<i64, 5>::default();
        ms.update_state::<_, BaseCurrency<i64, 5>>(
            &Bba {
                bid: QuoteCurrency::new(bid, 0),
                ask: QuoteCurrency::new(ask, 0),
                timestamp_exchange_ns: 1.into(),
            },
            &PriceFilter::default(),
        );
        ms
    }

    #[test]
    fn stop_trigger_band_uses_the_trade_range_only_for_last_stops() {
        let mut ms = quoted_market(98, 100);
        ms.set_fair_price(Some(QuoteCurrency::new(105, 0)));
        let range = Some((QuoteCurrency::new(80, 0), QuoteCurrency::new(120, 0)));
        let point = |p: i64| Some((QuoteCurrency::new(p, 0), QuoteCurrency::new(p, 0)));

        let band = |m, t, s| stop_trigger_band(&ms, m, t, s, range);
        assert_eq!(
            band(MarkMethod::BidAsk, TriggerPriceType::Last, Side::Sell),
            range
        );
        assert_eq!(
            band(MarkMethod::FairPrice, TriggerPriceType::Mark, Side::Sell),
            point(105)
        );
        assert_eq!(
            band(MarkMethod::MidPrice, TriggerPriceType::Mark, Side::Buy),
            point(99)
        );
        assert_eq!(
            band(
                MarkMethod::FairPrice,
                TriggerPriceType::BestBidAsk,
                Side::Sell
            ),
            point(98)
        );
        assert_eq!(
            band(
                MarkMethod::FairPrice,
                TriggerPriceType::BestBidAsk,
                Side::Buy
            ),
            point(100)
        );
        // Without a candle range a `Last` stop falls back to the mid price.
        assert_eq!(
            stop_trigger_band(
                &ms,
                MarkMethod::BidAsk,
                TriggerPriceType::Last,
                Side::Sell,
                None
            ),
            point(99)
        );
        // No quotes yet: no reference price, so nothing can be elected.
        assert_eq!(
            stop_trigger_band(
                &MarketState::<i64, 5>::default(),
                MarkMethod::BidAsk,
                TriggerPriceType::BestBidAsk,
                Side::Sell,
                range
            ),
            None
        );
        assert!(elected_intrabar(TriggerPriceType::Last, range));
        assert!(!elected_intrabar(TriggerPriceType::Mark, range));
        assert!(!elected_intrabar(TriggerPriceType::BestBidAsk, range));
        assert!(!elected_intrabar::<i64, 5>(TriggerPriceType::Last, None));
    }

    #[test]
    fn capped_clearance_fee_never_exceeds_the_equity_left() {
        let fee = QuoteCurrency::<i64, 5>::new(47, 0);
        assert_eq!(
            capped_clearance_fee(fee, QuoteCurrency::new(100, 0)),
            QuoteCurrency::new(47, 0)
        );
        assert_eq!(
            capped_clearance_fee(fee, QuoteCurrency::new(47, 0)),
            QuoteCurrency::new(47, 0)
        );
        assert_eq!(
            capped_clearance_fee(fee, QuoteCurrency::new(30, 0)),
            QuoteCurrency::new(30, 0)
        );
        assert_eq!(
            capped_clearance_fee(fee, QuoteCurrency::zero()),
            QuoteCurrency::zero()
        );
        assert_eq!(
            capped_clearance_fee(fee, QuoteCurrency::new(-5, 0)),
            QuoteCurrency::zero()
        );
    }

    #[test]
    fn marketable_limit_fill_price_applies_slippage_capped_at_limit() {
        let mut ms = MarketState::<i64, 5>::default();
        ms.update_state::<_, BaseCurrency<i64, 5>>(
            &Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into(),
            },
            &PriceFilter::default(),
        );
        let slip = SlippageModel::fixed_rate(
            const_decimal::Decimal::try_from_scaled(1, 2).expect("valid decimal"),
        )
        .expect("valid slippage");
        let qty = BaseCurrency::<i64, 5>::new(1, 0);
        let tick = QuoteCurrency::new(1, 2);
        // 101 * 1.01 = 102.01 below the limit.
        assert_eq!(
            marketable_limit_fill_price(
                slip,
                tick,
                &ms,
                Side::Buy,
                qty,
                QuoteCurrency::new(105, 0)
            ),
            QuoteCurrency::new(10201, 2)
        );
        // Capped at the limit.
        assert_eq!(
            marketable_limit_fill_price(
                slip,
                tick,
                &ms,
                Side::Buy,
                qty,
                QuoteCurrency::new(1015, 1)
            ),
            QuoteCurrency::new(1015, 1)
        );
        // 100 * 0.99 = 99 above the limit.
        assert_eq!(
            marketable_limit_fill_price(
                slip,
                tick,
                &ms,
                Side::Sell,
                qty,
                QuoteCurrency::new(95, 0)
            ),
            QuoteCurrency::new(99, 0)
        );
        assert_eq!(
            marketable_limit_fill_price(
                slip,
                tick,
                &ms,
                Side::Sell,
                qty,
                QuoteCurrency::new(995, 1)
            ),
            QuoteCurrency::new(995, 1)
        );
    }
}
