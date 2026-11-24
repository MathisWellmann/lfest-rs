//! Order admission and execution rules shared by [`crate::prelude::Exchange`] and
//! [`crate::prelude::PortfolioExchange`].
//!
//! Both venues implement the same order semantics on different account models. Keeping
//! the rules here as pure functions means a fix cannot land in only one of them.

use num_traits::Zero;

use crate::{
    market_state::MarketState,
    order_filters::NotionalFilter,
    slippage::SlippageModel,
    types::{
        Currency,
        LimitOrder,
        MarginCurrency,
        Mon,
        NewOrder,
        QuoteCurrency,
        Side,
        SubmitLimitOrderError,
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
        .quote(market_state, side, quantity)
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
    if !reduce_only {
        return filter.validate_order_notional(notional);
    }
    if let Some(max_notional) = filter.max_notional()
        && notional > max_notional
    {
        return Err(ValidateOrderNotionalError::NotionalTooHigh);
    }
    Ok(())
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
    let capped = if equity_left > Margin::zero() {
        crate::utils::min(clearance_fee, equity_left)
    } else {
        Margin::zero()
    };
    assert!(capped >= Margin::zero());
    assert!(capped <= clearance_fee);
    if equity_left >= Margin::zero() {
        assert!(capped <= equity_left);
    }
    capped
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
        // 101 * 1.01 = 102.01 below the limit.
        assert_eq!(
            marketable_limit_fill_price(slip, &ms, Side::Buy, qty, QuoteCurrency::new(105, 0)),
            QuoteCurrency::new(10201, 2)
        );
        // Capped at the limit.
        assert_eq!(
            marketable_limit_fill_price(slip, &ms, Side::Buy, qty, QuoteCurrency::new(1015, 1)),
            QuoteCurrency::new(1015, 1)
        );
        // 100 * 0.99 = 99 above the limit.
        assert_eq!(
            marketable_limit_fill_price(slip, &ms, Side::Sell, qty, QuoteCurrency::new(95, 0)),
            QuoteCurrency::new(99, 0)
        );
        assert_eq!(
            marketable_limit_fill_price(slip, &ms, Side::Sell, qty, QuoteCurrency::new(995, 1)),
            QuoteCurrency::new(995, 1)
        );
    }
}
