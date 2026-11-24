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
        let elected_before = elected_out.len();
        let pending_before = self.stops.len();
        // `retain_mut` compacts in one O(n) pass and keeps the survivors in admission
        // order, unlike repeated `Vec::remove` (O(n^2)) or `swap_remove` (reorders).
        self.stops.retain_mut(|stop| {
            let Some((low, high)) = price_band_for(stop) else {
                return true;
            };
            let Some(trig) = Self::trail_and_test(stop, low, high) else {
                return true;
            };
            elected_out
                .push_within_capacity((*stop, trig))
                .expect(EXPECT_CAPACITY);
            false
        });
        assert_eq!(
            pending_before - self.stops.len(),
            elected_out.len() - elected_before,
            "every removed stop must be elected exactly once"
        );

        // Deterministic priority: stop-loss/trailing before take-profit, then tighter
        // stop price first, then admission OrderId. The trigger direction is compared
        // before the direction-dependent price key so the comparator is a total order
        // even when stops of both directions are elected together (a comparator choosing
        // the price direction from one operand only is inconsistent and can make the
        // sort panic). Ids are unique, so an unstable sort is deterministic and avoids
        // the scratch allocation of the stable sort.
        elected_out.sort_unstable_by(|(a, _), (b, _)| {
            Self::is_take_profit(a)
                .cmp(&Self::is_take_profit(b))
                .then_with(|| Self::direction_rank(a).cmp(&Self::direction_rank(b)))
                .then_with(|| match a.trigger_direction() {
                    TriggerDirection::AtOrBelow => b.stop_price().cmp(&a.stop_price()),
                    TriggerDirection::AtOrAbove => a.stop_price().cmp(&b.stop_price()),
                })
                .then_with(|| a.id().cmp(&b.id()))
        });
        debug_assert!(elected_out.windows(2).all(|w| w[0].0.id() != w[1].0.id()));
    }

    /// Trail `stop` and return the trigger price if the observation `(low, high)`
    /// elects it.
    ///
    /// For single-point prints, trail before testing (matching `OrderBook-rs`).
    /// For multi-price candle ranges, test the adverse extreme first conservatively and
    /// only trail to the favorable extreme if the stop survived the adverse one.
    #[inline]
    fn trail_and_test(
        stop: &mut StopOrder<I, D, BaseOrQuote, UserOrderIdT>,
        low: QuoteCurrency<I, D>,
        high: QuoteCurrency<I, D>,
    ) -> Option<QuoteCurrency<I, D>> {
        debug_assert!(low <= high, "price band must be ordered");
        if low >= high {
            stop.trail_to(low);
            return stop.is_triggered_by(low).then_some(low);
        }
        let test_price = match stop.trigger_direction() {
            TriggerDirection::AtOrBelow => low,
            TriggerDirection::AtOrAbove => high,
        };
        if stop.is_triggered_by(test_price) {
            return Some(test_price);
        }
        let favorable = match stop.side() {
            Side::Sell => high,
            Side::Buy => low,
        };
        stop.trail_to(favorable);
        None
    }

    #[inline]
    fn is_take_profit(stop: &StopOrder<I, D, BaseOrQuote, UserOrderIdT>) -> bool {
        matches!(
            stop.kind(),
            StopKind::TakeProfitMarket | StopKind::TakeProfitLimit { .. }
        )
    }

    /// Arbitrary but fixed rank making the sort key total across trigger directions.
    #[inline]
    fn direction_rank(stop: &StopOrder<I, D, BaseOrQuote, UserOrderIdT>) -> u8 {
        match stop.trigger_direction() {
            TriggerDirection::AtOrBelow => 0,
            TriggerDirection::AtOrAbove => 1,
        }
    }

    /// Look up a pending stop by `OrderId`.
    #[inline]
    #[must_use]
    pub fn get_by_id(&self, id: OrderId) -> Option<&StopOrder<I, D, BaseOrQuote, UserOrderIdT>> {
        self.stops.iter().find(|s| s.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DECIMALS,
        prelude::{
            BaseCurrency,
            TimeInForce,
        },
        utils::NoUserOrderId,
    };

    type Stop = StopOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>;

    /// Admit 32 stops of all kinds and both trigger directions around a price of 100,
    /// interleaving the kinds so admission order differs from priority order.
    fn mixed_stops() -> PendingStops<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
        let mut store = PendingStops::with_capacity(NonZeroU16::new(64).unwrap());
        let qty = BaseCurrency::new(1, 0);
        let mut id = 0_u64;
        for i in 0..8_i64 {
            let below = QuoteCurrency::new(99 - i, 0);
            let above = QuoteCurrency::new(101 + i, 0);
            let stops: [Stop; 4] = [
                Stop::take_profit_market(Side::Sell, above, qty, NoUserOrderId).unwrap(),
                Stop::stop_market(Side::Sell, below, qty, NoUserOrderId).unwrap(),
                Stop::take_profit_limit(
                    Side::Buy,
                    below,
                    below,
                    qty,
                    TimeInForce::GoodTilCancelled,
                    NoUserOrderId,
                )
                .unwrap(),
                Stop::stop_limit(
                    Side::Buy,
                    above,
                    above,
                    qty,
                    TimeInForce::GoodTilCancelled,
                    NoUserOrderId,
                )
                .unwrap(),
            ];
            for mut stop in stops {
                id += 1;
                stop.stamp_admitted(OrderId::from(id), 0.into());
                store.try_insert(stop).unwrap();
            }
        }
        store
    }

    #[test]
    fn mixed_direction_election_is_totally_ordered() {
        let mut store = mixed_stops();
        assert_eq!(store.len(), 32);
        let mut elected = Vec::with_capacity(64);
        // One wide candle elects every stop in both directions.
        store.evaluate_and_drain(
            |_| Some((QuoteCurrency::new(50, 0), QuoteCurrency::new(150, 0))),
            &mut elected,
        );
        assert!(store.is_empty());
        assert_eq!(elected.len(), 32);

        let n_sl = elected
            .iter()
            .take_while(|(s, _)| !PendingStops::is_take_profit(s))
            .count();
        assert_eq!(n_sl, 16, "all stop-losses come before take-profits");
        for group in [&elected[..n_sl], &elected[n_sl..]] {
            let (below, above) = group.split_at(8);
            assert!(
                below
                    .iter()
                    .all(|(s, _)| s.trigger_direction() == TriggerDirection::AtOrBelow)
            );
            assert!(
                above
                    .iter()
                    .all(|(s, _)| s.trigger_direction() == TriggerDirection::AtOrAbove)
            );
            // Tighter (closer to the market) stop first within each direction.
            assert!(
                below
                    .windows(2)
                    .all(|w| w[0].0.stop_price() > w[1].0.stop_price())
            );
            assert!(
                above
                    .windows(2)
                    .all(|w| w[0].0.stop_price() < w[1].0.stop_price())
            );
        }

        // Deterministic: the same input always yields the same order.
        let mut again = Vec::with_capacity(64);
        mixed_stops().evaluate_and_drain(
            |_| Some((QuoteCurrency::new(50, 0), QuoteCurrency::new(150, 0))),
            &mut again,
        );
        assert_eq!(elected, again);
    }

    #[test]
    fn partial_election_keeps_survivors_in_admission_order() {
        let mut store = mixed_stops();
        let mut elected = Vec::with_capacity(64);
        // A point print at 97 elects the AtOrBelow stops at 97, 98 and 99 (both kinds).
        store.evaluate_and_drain(
            |_| Some((QuoteCurrency::new(97, 0), QuoteCurrency::new(97, 0))),
            &mut elected,
        );
        assert_eq!(elected.len(), 6);
        assert_eq!(store.len(), 26);
        let ids: Vec<OrderId> = store.iter().map(|s| s.id()).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        // Equal stop prices fall back to the admission id.
        assert!(elected[..3].windows(2).all(|w| {
            w[0].0.stop_price() > w[1].0.stop_price()
                || (w[0].0.stop_price() == w[1].0.stop_price() && w[0].0.id() < w[1].0.id())
        }));
    }
}
