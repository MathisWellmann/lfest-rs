//! Single-threaded, pre-allocated, lazy `PendingStops` store (adapted from `OrderBook-rs`).
//!
//! When empty, [`PendingStops::is_empty`] lets `update_state` skip all stop-evaluation
//! work in a single branch with zero overhead.

use std::num::NonZeroU16;

use super::{
    StopKind,
    StopOrder,
    StopOrderError,
    TriggerDirection,
};
use crate::{
    EXPECT_CAPACITY,
    types::{
        CancelBy,
        Currency,
        Mon,
        OrderId,
        QuoteCurrency,
        Side,
        UserOrderId,
    },
};

/// An elected stop order paired with the reference price that triggered it.
pub type ElectedStop<I, const D: u8, BaseOrQuote, UserOrderIdT> = (
    StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
    QuoteCurrency<I, D>,
);

/// Single-threaded off-book store holding active pending conditional / stop orders.
///
/// Capacity is pre-allocated at construction so admission and election never allocate
/// on the hot path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingStops<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    stops: Vec<StopOrder<I, D, BaseOrQuote, UserOrderIdT>>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> PendingStops<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create an empty store with pre-allocated capacity `max_pending_stops`.
    #[must_use]
    pub fn with_capacity(max_pending_stops: NonZeroU16) -> Self {
        Self {
            stops: Vec::with_capacity(usize::from(max_pending_stops.get())),
        }
    }

    /// Number of currently pending stop orders.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.stops.len()
    }

    /// `true` when no stop orders are pending (fast-path guard on `update_state`).
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }

    /// Iterate over all pending stop orders in admission order.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &StopOrder<I, D, BaseOrQuote, UserOrderIdT>> {
        self.stops.iter()
    }

    /// Insert a new pending stop order if capacity allows.
    ///
    /// # Errors
    /// Returns [`StopOrderError::MaxPendingStops`] if full.
    #[inline]
    pub fn try_insert(
        &mut self,
        order: StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
    ) -> Result<(), StopOrderError> {
        if self.stops.len() >= self.stops.capacity() {
            let cap = u16::try_from(self.stops.capacity()).expect("capacity fits in u16");
            return Err(StopOrderError::MaxPendingStops(cap));
        }
        self.stops
            .push_within_capacity(order)
            .expect(EXPECT_CAPACITY);
        Ok(())
    }

    /// Cancel and return a pending stop order by [`CancelBy`].
    ///
    /// # Errors
    /// Returns [`StopOrderError::NotFound`] if no matching pending stop exists.
    pub fn remove(
        &mut self,
        cancel_by: CancelBy<UserOrderIdT>,
    ) -> Result<StopOrder<I, D, BaseOrQuote, UserOrderIdT>, StopOrderError> {
        let pos = match cancel_by {
            CancelBy::OrderId(oid) => self.stops.iter().position(|s| s.id() == oid),
            CancelBy::UserOrderId(uid) => self.stops.iter().position(|s| s.user_order_id() == uid),
        };
        let Some(idx) = pos else {
            return Err(StopOrderError::NotFound);
        };
        Ok(self.stops.remove(idx))
    }

    /// Cancel and drain every pending stop order into `out`.
    pub fn drain_all(&mut self, out: &mut Vec<StopOrder<I, D, BaseOrQuote, UserOrderIdT>>) {
        for stop in self.stops.drain(..) {
            out.push_within_capacity(stop).expect(EXPECT_CAPACITY);
        }
    }

    /// Trail watermarks and extract every stop elected by a single-point price observation
    /// `(ref_price_for_stop)` or candle price range `(low, high)` into `elected_out`
    /// along with the conservative trigger price that elected each stop.
    ///
    /// Ordering semantics (adapted from `OrderBook-rs` `stop_orders.rs` + conservative bar model):
    /// 1. For point observations (`low == high == p`), trail watermarks at `p` first, then
    ///    elect any stop crossed by `p`.
    /// 2. For candle intervals (`low < high`), conservatively test stop-loss / trailing stops
    ///    against the adverse extreme *before* trailing them to the favorable extreme, so a
    ///    bar that hits both extremes does not optimistically trail a stop out of harm's way.
    ///    Take-profits are tested against the favorable extreme.
    /// 3. Elected stops are sorted deterministically: stop-losses before take-profits,
    ///    highest stop price first for `AtOrBelow`, lowest first for `AtOrAbove`, ties broken
    ///    by admission `OrderId`.
    pub fn evaluate_and_drain<F>(
        &mut self,
        mut price_band_for: F,
        elected_out: &mut Vec<ElectedStop<I, D, BaseOrQuote, UserOrderIdT>>,
    ) where
        F: FnMut(
            &StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        ) -> Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)>,
    {
        if self.stops.is_empty() {
            return;
        }
        let mut idx = 0;
        while idx < self.stops.len() {
            let Some((low, high)) = price_band_for(&self.stops[idx]) else {
                idx += 1;
                continue;
            };
            let is_range = low < high;
            let stop = &mut self.stops[idx];

            // For single-point prints, trail before testing (matching OrderBook-rs).
            // For multi-price candle ranges, test adverse extreme first conservatively;
            // only trail if the stop survived the adverse extreme.
            let trigger_price = if !is_range {
                stop.trail_to(low);
                if stop.is_triggered_by(low) {
                    Some(low)
                } else {
                    None
                }
            } else {
                let test_price = match stop.trigger_direction() {
                    TriggerDirection::AtOrBelow => low,
                    TriggerDirection::AtOrAbove => high,
                };
                if stop.is_triggered_by(test_price) {
                    Some(test_price)
                } else {
                    // Survived the adverse extreme; trail using favorable extreme.
                    let favorable = match stop.side() {
                        Side::Sell => high,
                        Side::Buy => low,
                    };
                    stop.trail_to(favorable);
                    None
                }
            };

            if let Some(trig) = trigger_price {
                let elected = self.stops.remove(idx);
                elected_out
                    .push_within_capacity((elected, trig))
                    .expect(EXPECT_CAPACITY);
            } else {
                idx += 1;
            }
        }

        // Deterministic priority: stop-loss/trailing before take-profit, then tighter
        // stop price first, then admission OrderId.
        elected_out.sort_by(|(a, _), (b, _)| {
            let a_is_tp = matches!(
                a.kind(),
                StopKind::TakeProfitMarket | StopKind::TakeProfitLimit { .. }
            );
            let b_is_tp = matches!(
                b.kind(),
                StopKind::TakeProfitMarket | StopKind::TakeProfitLimit { .. }
            );
            a_is_tp
                .cmp(&b_is_tp)
                .then_with(|| match a.trigger_direction() {
                    TriggerDirection::AtOrBelow => b.stop_price().cmp(&a.stop_price()),
                    TriggerDirection::AtOrAbove => a.stop_price().cmp(&b.stop_price()),
                })
                .then_with(|| a.id().cmp(&b.id()))
        });
    }

    /// Look up a pending stop by `OrderId`.
    #[inline]
    #[must_use]
    pub fn get_by_id(&self, id: OrderId) -> Option<&StopOrder<I, D, BaseOrQuote, UserOrderIdT>> {
        self.stops.iter().find(|s| s.id() == id)
    }
}
