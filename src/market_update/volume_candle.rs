//! Volume-aware, participation-capped candle market update.
//!
//! Unlike [`crate::market_update::Candle`] (which assumes infinite volume at any crossed
//! limit price), [`VolumeCandle`] tracks remaining buy/sell taker capacity derived from
//! `bar_volume * participation_rate`, decrementing available capacity as resting limit
//! orders fill within the bar.

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

use super::{
    Candle,
    MarketUpdate,
    candle_update::NewCandleError,
};
use crate::{
    market_update::market_update_trait::Exhausted,
    prelude::{
        Currency,
        LimitOrder,
        MarketState,
        Mon,
        Pending,
        PriceFilter,
        QuoteCurrency,
        Side,
    },
    types::{
        PriceFilterError,
        TimestampNs,
        UserOrderId,
    },
    utils::min,
};

/// A candle update that caps total passive limit-order fill quantity to a configured
/// fraction (`participation_rate`) of the bar's traded volume.
#[derive(Debug, Clone, Copy, Eq, PartialEq, CopyGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VolumeCandle<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// The prices and timestamp, which decide which orders can fill at all.
    #[getset(get_copy = "pub")]
    candle: Candle<I, D>,

    /// Total bar volume reported by the feed.
    #[getset(get_copy = "pub")]
    bar_volume: BaseOrQuote,

    /// Remaining fillable bid capacity (derived from `bar_volume * participation_rate`).
    #[getset(get_copy = "pub")]
    remaining_bid_fill_volume: BaseOrQuote,

    /// Remaining fillable ask capacity (derived from `bar_volume * participation_rate`).
    #[getset(get_copy = "pub")]
    remaining_ask_fill_volume: BaseOrQuote,
}

impl<I, const D: u8, BaseOrQuote> VolumeCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Create a new [`VolumeCandle`] with symmetric buy/sell fill capacity equal to
    /// `bar_volume * participation_rate`.
    ///
    /// # Errors
    /// Returns [`NewCandleError`] if the price relationships violate candle invariants.
    ///
    /// # Panics
    /// Panics if `bar_volume < 0` or `participation_rate` is outside `[0, 1]`.
    pub fn new(
        bid: QuoteCurrency<I, D>,
        ask: QuoteCurrency<I, D>,
        low: QuoteCurrency<I, D>,
        high: QuoteCurrency<I, D>,
        bar_volume: BaseOrQuote,
        participation_rate: Decimal<I, D>,
        timestamp_exchange_ns: TimestampNs,
    ) -> Result<Self, NewCandleError> {
        let candle = Candle::new(bid, ask, low, high, timestamp_exchange_ns)?;
        assert!(
            bar_volume >= BaseOrQuote::zero(),
            "bar_volume must be non-negative"
        );
        assert!(
            participation_rate >= Decimal::zero(),
            "participation_rate must lie in [0, 1]"
        );
        assert!(
            participation_rate <= Decimal::one(),
            "participation_rate must lie in [0, 1]"
        );

        let cap = bar_volume * participation_rate;
        assert!(cap <= bar_volume);
        Ok(Self {
            candle,
            bar_volume,
            remaining_bid_fill_volume: cap,
            remaining_ask_fill_volume: cap,
        })
    }

    /// Best bid at candle close.
    #[inline]
    #[must_use]
    pub fn bid(&self) -> QuoteCurrency<I, D> {
        self.candle.bid()
    }

    /// Best ask at candle close.
    #[inline]
    #[must_use]
    pub fn ask(&self) -> QuoteCurrency<I, D> {
        self.candle.ask()
    }

    /// Candle low price.
    #[inline]
    #[must_use]
    pub fn low(&self) -> QuoteCurrency<I, D> {
        self.candle.low()
    }

    /// Candle high price.
    #[inline]
    #[must_use]
    pub fn high(&self) -> QuoteCurrency<I, D> {
        self.candle.high()
    }

    /// Exchange timestamp in nanoseconds.
    #[inline]
    #[must_use]
    pub fn timestamp_exchange_ns(&self) -> TimestampNs {
        self.candle.timestamp_exchange_ns()
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for VolumeCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VolumeCandle({}, vol: {})", self.candle, self.bar_volume)
    }
}

impl<I, const D: u8, BaseOrQuote> MarketUpdate<I, D, BaseOrQuote>
    for VolumeCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const CAN_FILL_LIMIT_ORDERS: bool = true;

    #[inline]
    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &mut self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Option<(BaseOrQuote, Exhausted)> {
        debug_assert!(order.remaining_quantity() > BaseOrQuote::zero());
        if !self.candle.fills_limit_order(order) {
            return None;
        }
        let remaining = match order.side() {
            Side::Buy => &mut self.remaining_bid_fill_volume,
            Side::Sell => &mut self.remaining_ask_fill_volume,
        };
        if *remaining <= BaseOrQuote::zero() {
            return None;
        }
        let filled = min(*remaining, order.remaining_quantity());
        *remaining -= filled;
        assert!(*remaining >= BaseOrQuote::zero());
        assert!(filled > BaseOrQuote::zero());
        Some((filled, false))
    }

    #[inline]
    fn validate_market_update(
        &self,
        price_filter: &PriceFilter<I, D>,
    ) -> Result<(), PriceFilterError> {
        <Candle<I, D> as MarketUpdate<I, D, BaseOrQuote>>::validate_market_update(
            &self.candle,
            price_filter,
        )
    }

    #[inline]
    fn update_market_state(&self, market_state: &mut MarketState<I, D>) {
        <Candle<I, D> as MarketUpdate<I, D, BaseOrQuote>>::update_market_state(
            &self.candle,
            market_state,
        );
    }

    #[inline(always)]
    fn timestamp_exchange_ns(&self) -> TimestampNs {
        Self::timestamp_exchange_ns(self)
    }

    #[inline(always)]
    fn price_range(&self) -> Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)> {
        <Candle<I, D> as MarketUpdate<I, D, BaseOrQuote>>::price_range(&self.candle)
    }

    #[inline(always)]
    fn can_fill_bids(&self) -> bool {
        self.remaining_bid_fill_volume > BaseOrQuote::zero()
    }

    #[inline(always)]
    fn can_fill_asks(&self) -> bool {
        self.remaining_ask_fill_volume > BaseOrQuote::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        prelude::BaseCurrency,
        types::ExchangeOrderMeta,
        utils::NoUserOrderId,
    };

    type Base = BaseCurrency<i64, 5>;
    type Update = VolumeCandle<i64, 5, Base>;

    /// Bid/ask 100/101 within a 95..=105 bar, with 5 units of capacity per side.
    fn volume_candle(participation_pct: i64) -> Update {
        VolumeCandle::new(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            QuoteCurrency::new(95, 0),
            QuoteCurrency::new(105, 0),
            BaseCurrency::new(50, 0),
            Decimal::try_from_scaled(participation_pct, 2).unwrap(),
            7.into(),
        )
        .unwrap()
    }

    fn order(
        side: Side,
        price: i64,
        qty: i64,
    ) -> LimitOrder<i64, 5, Base, NoUserOrderId, Pending<i64, 5, Base>> {
        LimitOrder::new(
            side,
            QuoteCurrency::new(price, 0),
            BaseCurrency::new(qty, 0),
        )
        .unwrap()
        .into_pending(ExchangeOrderMeta::new(0.into(), 1.into()))
    }

    fn can_fill(vc: &Update, side: Side) -> bool {
        match side {
            Side::Buy => vc.can_fill_bids(),
            Side::Sell => vc.can_fill_asks(),
        }
    }

    #[test]
    fn volume_candle_rejects_invalid_prices_like_candle() {
        let q = |p| QuoteCurrency::<i64, 5>::new(p, 0);
        let new = |bid, ask, low, high| {
            Update::new(
                q(bid),
                q(ask),
                q(low),
                q(high),
                Base::new(1, 0),
                Decimal::ONE,
                0.into(),
            )
        };
        assert_eq!(new(94, 101, 95, 105), Err(NewCandleError::BidBelowLow));
        assert_eq!(new(100, 106, 95, 105), Err(NewCandleError::AskAboveHigh));
        assert_eq!(new(100, 94, 95, 105), Err(NewCandleError::AskBelowLow));
        assert_eq!(new(100, 101, 95, 94), Err(NewCandleError::HighBelowLow));
        assert!(new(95, 105, 95, 105).is_ok());
    }

    #[test]
    fn volume_candle_validates_all_prices_like_candle() {
        let vc = volume_candle(10);
        let validate = |filter: &PriceFilter<i64, 5>| {
            <Update as MarketUpdate<i64, 5, Base>>::validate_market_update(&vc, filter)
        };
        assert_eq!(validate(&PriceFilter::default()), Ok(()));
        // Only the low of 95 is below the minimum price.
        let min_96 = PriceFilter::new(
            Some(QuoteCurrency::new(96, 0)),
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::zero(),
        )
        .unwrap();
        assert!(validate(&min_96).is_err());
    }

    #[test]
    #[should_panic(expected = "participation_rate must lie in [0, 1]")]
    fn volume_candle_rejects_participation_above_one() {
        let _ = volume_candle(101);
    }

    #[test]
    fn volume_candle_exposes_the_candle_to_the_exchange() {
        let vc = volume_candle(10);
        let mut state = MarketState::default();
        <Update as MarketUpdate<i64, 5, Base>>::update_market_state(&vc, &mut state);
        assert_eq!(state.bid(), QuoteCurrency::new(100, 0));
        assert_eq!(state.ask(), QuoteCurrency::new(101, 0));
        assert_eq!(
            <Update as MarketUpdate<i64, 5, Base>>::timestamp_exchange_ns(&vc),
            7.into()
        );
        assert_eq!(
            <Update as MarketUpdate<i64, 5, Base>>::price_range(&vc),
            Some((QuoteCurrency::new(95, 0), QuoteCurrency::new(105, 0)))
        );
        assert_eq!(
            (
                vc.bid(),
                vc.ask(),
                vc.low(),
                vc.high(),
                vc.timestamp_exchange_ns()
            ),
            (
                QuoteCurrency::new(100, 0),
                QuoteCurrency::new(101, 0),
                QuoteCurrency::new(95, 0),
                QuoteCurrency::new(105, 0),
                7.into()
            )
        );
        assert_eq!(vc.bar_volume(), BaseCurrency::new(50, 0));
        assert_eq!(
            vc.to_string(),
            "VolumeCandle(bid: 100.00000 Quote, ask: 101.00000 Quote, high: 105.00000 Quote, low: 95.00000 Quote, vol: 50.00000 Base)"
        );
    }

    #[test_case::test_matrix([Side::Buy, Side::Sell])]
    fn volume_candle_caps_fills_per_side(side: Side) {
        let mut vc = volume_candle(10);
        // A price inside the bar, on the side where the bar crosses it.
        let inside = match side {
            Side::Buy => 99,
            Side::Sell => 102,
        };
        assert_eq!(
            vc.limit_order_filled(&order(side, inside, 4)),
            Some((Base::new(4, 0), false))
        );
        assert_eq!(
            vc.limit_order_filled(&order(side, inside, 4)),
            Some((Base::new(1, 0), false))
        );
        assert!(!can_fill(&vc, side));
        assert_eq!(vc.limit_order_filled(&order(side, inside, 1)), None);

        // The other side keeps its full capacity.
        let other = side.inverted();
        assert!(can_fill(&vc, other));
        let other_price = match other {
            Side::Buy => 99,
            Side::Sell => 102,
        };
        assert_eq!(
            vc.limit_order_filled(&order(other, other_price, 9)),
            Some((Base::new(5, 0), false))
        );
        assert_eq!(vc.remaining_bid_fill_volume(), Base::zero());
        assert_eq!(vc.remaining_ask_fill_volume(), Base::zero());
    }

    #[test]
    fn volume_candle_does_not_fill_at_or_beyond_the_bar_extremes() {
        let mut vc = volume_candle(10);
        // Touching the low or high is not enough, as the queue position is unknown.
        assert_eq!(vc.limit_order_filled(&order(Side::Buy, 95, 1)), None);
        assert_eq!(vc.limit_order_filled(&order(Side::Sell, 105, 1)), None);
        assert_eq!(vc.limit_order_filled(&order(Side::Sell, 110, 1)), None);
        assert_eq!(vc.remaining_bid_fill_volume(), Base::new(5, 0));
        assert_eq!(vc.remaining_ask_fill_volume(), Base::new(5, 0));
        assert_eq!(
            vc.limit_order_filled(&order(Side::Buy, 96, 1)),
            Some((Base::new(1, 0), false))
        );
        assert_eq!(
            vc.limit_order_filled(&order(Side::Sell, 104, 1)),
            Some((Base::new(1, 0), false))
        );
    }

    #[test]
    fn volume_candle_without_participation_fills_nothing() {
        let mut vc = volume_candle(0);
        assert!(!can_fill(&vc, Side::Buy));
        assert!(!can_fill(&vc, Side::Sell));
        assert_eq!(vc.limit_order_filled(&order(Side::Buy, 99, 1)), None);
        assert_eq!(vc.limit_order_filled(&order(Side::Sell, 102, 1)), None);
    }
}
