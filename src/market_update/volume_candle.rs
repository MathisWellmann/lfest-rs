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
    MarketUpdate,
    candle_update::NewCandleError,
};
use crate::{
    market_update::market_update_trait::Exhausted,
    order_filters::{
        enforce_bid_ask_spread,
        enforce_max_price,
        enforce_min_price,
        enforce_step_size,
    },
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
    /// Best bid at candle close.
    #[getset(get_copy = "pub")]
    bid: QuoteCurrency<I, D>,

    /// Best ask at candle close.
    #[getset(get_copy = "pub")]
    ask: QuoteCurrency<I, D>,

    /// Candle low price.
    #[getset(get_copy = "pub")]
    low: QuoteCurrency<I, D>,

    /// Candle high price.
    #[getset(get_copy = "pub")]
    high: QuoteCurrency<I, D>,

    /// Total bar volume reported by the feed.
    #[getset(get_copy = "pub")]
    bar_volume: BaseOrQuote,

    /// Remaining fillable bid capacity (derived from `bar_volume * participation_rate`).
    #[getset(get_copy = "pub")]
    remaining_bid_fill_volume: BaseOrQuote,

    /// Remaining fillable ask capacity (derived from `bar_volume * participation_rate`).
    #[getset(get_copy = "pub")]
    remaining_ask_fill_volume: BaseOrQuote,

    /// Exchange timestamp in nanoseconds.
    #[getset(get_copy = "pub")]
    timestamp_exchange_ns: TimestampNs,
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
        if bid < low {
            return Err(NewCandleError::BidBelowLow);
        }
        if ask < low {
            return Err(NewCandleError::AskBelowLow);
        }
        if high < low {
            return Err(NewCandleError::HighBelowLow);
        }
        if bid > high {
            return Err(NewCandleError::BidAboveHigh);
        }
        if ask > high {
            return Err(NewCandleError::AskAboveHigh);
        }
        assert!(
            bar_volume >= BaseOrQuote::zero(),
            "bar_volume must be non-negative"
        );
        assert!(
            participation_rate >= Decimal::zero() && participation_rate <= Decimal::one(),
            "participation_rate must lie in [0, 1]"
        );

        let cap = bar_volume * participation_rate;
        Ok(Self {
            bid,
            ask,
            low,
            high,
            bar_volume,
            remaining_bid_fill_volume: cap,
            remaining_ask_fill_volume: cap,
            timestamp_exchange_ns,
        })
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for VolumeCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VolumeCandle(bid: {}, ask: {}, low: {}, high: {}, vol: {})",
            self.bid, self.ask, self.low, self.high, self.bar_volume
        )
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
        match order.side() {
            Side::Buy => {
                if self.low >= order.limit_price()
                    || self.remaining_bid_fill_volume <= BaseOrQuote::zero()
                {
                    return None;
                }
                let filled = min(self.remaining_bid_fill_volume, order.remaining_quantity());
                self.remaining_bid_fill_volume -= filled;
                Some((filled, false))
            }
            Side::Sell => {
                if self.high <= order.limit_price()
                    || self.remaining_ask_fill_volume <= BaseOrQuote::zero()
                {
                    return None;
                }
                let filled = min(self.remaining_ask_fill_volume, order.remaining_quantity());
                self.remaining_ask_fill_volume -= filled;
                Some((filled, false))
            }
        }
    }

    fn validate_market_update(
        &self,
        price_filter: &PriceFilter<I, D>,
    ) -> Result<(), PriceFilterError> {
        enforce_min_price(price_filter.min_price(), self.bid)?;
        enforce_min_price(price_filter.min_price(), self.ask)?;
        enforce_min_price(price_filter.min_price(), self.low)?;
        enforce_min_price(price_filter.min_price(), self.high)?;
        enforce_max_price(price_filter.max_price(), self.bid)?;
        enforce_max_price(price_filter.max_price(), self.ask)?;
        enforce_max_price(price_filter.max_price(), self.low)?;
        enforce_max_price(price_filter.max_price(), self.high)?;
        enforce_step_size(price_filter.tick_size(), self.bid)?;
        enforce_step_size(price_filter.tick_size(), self.ask)?;
        enforce_step_size(price_filter.tick_size(), self.low)?;
        enforce_step_size(price_filter.tick_size(), self.high)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        enforce_bid_ask_spread(self.low, self.high)?;
        Ok(())
    }

    #[inline]
    fn update_market_state(&self, market_state: &mut MarketState<I, D>) {
        market_state.set_bid(self.bid);
        market_state.set_ask(self.ask);
    }

    #[inline(always)]
    fn timestamp_exchange_ns(&self) -> TimestampNs {
        self.timestamp_exchange_ns
    }

    #[inline(always)]
    fn price_range(&self) -> Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)> {
        Some((self.low, self.high))
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
