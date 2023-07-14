use fpdec::Decimal;

use crate::{
    prelude::PriceFilter,
    quote,
    types::{Currency, MarketUpdate, QuoteCurrency, Result},
};

#[derive(Debug, Clone)]
pub struct MarketState {
    price_filter: PriceFilter,
    bid: QuoteCurrency,
    ask: QuoteCurrency,
    high: QuoteCurrency,
    low: QuoteCurrency,
    // The current timestamp in nanoseconds
    current_ts_ns: i64,
    step: u64, // used for synchronizing orders
}

impl MarketState {
    pub(crate) fn new(price_filter: PriceFilter) -> Self {
        Self {
            price_filter,
            bid: quote!(0),
            ask: quote!(0),
            high: quote!(0),
            low: quote!(0),
            current_ts_ns: 0,
            step: 0,
        }
    }

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    pub(crate) fn update_state(
        &mut self,
        timestamp_ns: u64,
        market_update: &MarketUpdate,
    ) -> Result<()> {
        self.price_filter.validate_market_update(market_update)?;

        match market_update {
            MarketUpdate::Bba { bid, ask } => {
                self.bid = *bid;
                self.ask = *ask;
                self.high = *ask;
                self.low = *bid;
            }
            MarketUpdate::Candle {
                bid,
                ask,
                high,
                low,
            } => {
                self.bid = *bid;
                self.ask = *ask;
                self.high = *high;
                self.low = *low;
            }
        }
        self.current_ts_ns = timestamp_ns as i64;
        self.step += 1;

        Ok(())
    }

    #[inline]
    pub fn mid_price(&self) -> QuoteCurrency {
        (self.bid + self.ask) / quote!(2)
    }

    #[inline(always)]
    pub fn current_timestamp_ns(&self) -> i64 {
        self.current_ts_ns
    }

    #[inline(always)]
    pub fn bid(&self) -> QuoteCurrency {
        self.bid
    }

    #[inline(always)]
    pub fn ask(&self) -> QuoteCurrency {
        self.ask
    }
}
