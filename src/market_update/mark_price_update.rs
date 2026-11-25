//! Mark / fair price market update for `MarkMethod::FairPrice` valuation and margining.

use getset::CopyGetters;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use super::MarketUpdate;
use crate::{
    market_update::market_update_trait::Exhausted,
    order_filters::{
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
    },
    types::{
        PriceFilterError,
        TimestampNs,
        UserOrderId,
    },
};

/// An update to the contract's external mark / fair price (for example a venue
/// index-based fair price feed used for mark-to-market PnL, funding, and
/// liquidation checks).
///
/// This update never fills resting limit orders (`CAN_FILL_LIMIT_ORDERS = false`).
#[derive(Debug, Clone, Copy, Eq, PartialEq, CopyGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MarkPriceUpdate<I, const D: u8>
where
    I: Mon<D>,
{
    /// The updated fair / mark price.
    #[getset(get_copy = "pub")]
    pub mark_price: QuoteCurrency<I, D>,

    /// Optional index price accompanying the mark price update.
    #[getset(get_copy = "pub")]
    pub index_price: Option<QuoteCurrency<I, D>>,

    /// The nanosecond timestamp at which this event occurred at the exchange.
    #[getset(get_copy = "pub")]
    pub timestamp_exchange_ns: TimestampNs,
}

impl<I, const D: u8> MarkPriceUpdate<I, D>
where
    I: Mon<D>,
{
    /// Create a new `MarkPriceUpdate` with no index price attached.
    #[inline]
    #[must_use]
    pub fn new(mark_price: QuoteCurrency<I, D>, timestamp_exchange_ns: TimestampNs) -> Self {
        Self {
            mark_price,
            index_price: None,
            timestamp_exchange_ns,
        }
    }

    /// Create a new `MarkPriceUpdate` with both mark and index prices.
    #[inline]
    #[must_use]
    pub fn with_index_price(
        mark_price: QuoteCurrency<I, D>,
        index_price: QuoteCurrency<I, D>,
        timestamp_exchange_ns: TimestampNs,
    ) -> Self {
        Self {
            mark_price,
            index_price: Some(index_price),
            timestamp_exchange_ns,
        }
    }
}

impl<I, const D: u8> std::fmt::Display for MarkPriceUpdate<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mark_price: {}, ts: {}",
            self.mark_price, self.timestamp_exchange_ns
        )
    }
}

impl<I, const D: u8, BaseOrQuote> MarketUpdate<I, D, BaseOrQuote> for MarkPriceUpdate<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const CAN_FILL_LIMIT_ORDERS: bool = false;

    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &mut self,
        _limit_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Option<(BaseOrQuote, Exhausted)> {
        std::hint::cold_path();
        unreachable!(
            "MarkPriceUpdate can never fill limit orders because CAN_FILL_LIMIT_ORDERS is false."
        );
    }

    fn validate_market_update(
        &self,
        price_filter: &PriceFilter<I, D>,
    ) -> Result<(), PriceFilterError> {
        enforce_min_price(price_filter.min_price(), self.mark_price)?;
        enforce_max_price(price_filter.max_price(), self.mark_price)?;
        enforce_step_size(price_filter.tick_size(), self.mark_price)?;
        if let Some(index_price) = self.index_price {
            enforce_min_price(price_filter.min_price(), index_price)?;
            enforce_max_price(price_filter.max_price(), index_price)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn update_market_state(&self, market_state: &mut MarketState<I, D>) {
        market_state.set_fair_price(Some(self.mark_price));
        if let Some(index_price) = self.index_price {
            market_state.set_index_price(Some(index_price));
        }
    }

    #[inline(always)]
    fn timestamp_exchange_ns(&self) -> TimestampNs {
        self.timestamp_exchange_ns
    }

    #[inline(always)]
    fn can_fill_bids(&self) -> bool {
        false
    }

    #[inline(always)]
    fn can_fill_asks(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BaseCurrency;

    #[test]
    fn mark_price_update_sets_fair_and_index_price() {
        let mut state = MarketState::<i64, 2>::default();
        let pf = PriceFilter::default();
        let update = MarkPriceUpdate::with_index_price(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(99, 0),
            42.into(),
        );
        assert!(!<MarkPriceUpdate<i64, 2> as MarketUpdate<
            i64,
            2,
            BaseCurrency<i64, 2>,
        >>::can_fill_bids(&update));
        assert!(!<MarkPriceUpdate<i64, 2> as MarketUpdate<
            i64,
            2,
            BaseCurrency<i64, 2>,
        >>::can_fill_asks(&update));

        state.update_state::<_, BaseCurrency<_, 2>>(&update, &pf);
        assert_eq!(state.fair_price(), Some(QuoteCurrency::new(100, 0)));
        assert_eq!(state.index_price(), Some(QuoteCurrency::new(99, 0)));
        assert_eq!(state.current_timestamp_ns(), 42.into());
    }
}
