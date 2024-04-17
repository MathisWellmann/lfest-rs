use fpdec::Decimal;

use crate::{
    prelude::PriceFilter,
    quote,
    types::{Currency, MarketUpdate, QuoteCurrency, Result},
};

/// Some information regarding the state of the market.
#[derive(Debug, Clone)]
pub struct MarketState {
    /// Used to validate states
    // TODO: remove here and pass through were needed
    price_filter: PriceFilter,
    /// The current bid
    bid: QuoteCurrency,
    /// The current ask
    ask: QuoteCurrency,
    /// The current timestamp in nanoseconds
    current_ts_ns: i64,
    /// Used for synchronizing orders
    step: u64,
}

impl MarketState {
    pub(crate) fn new(price_filter: PriceFilter) -> Self {
        Self {
            price_filter,
            bid: quote!(0),
            ask: quote!(0),
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
    pub(crate) fn update_state<S>(
        &mut self,
        timestamp_ns: u64,
        market_update: &MarketUpdate<S>,
    ) -> Result<()>
    where
        S: Currency,
    {
        self.price_filter.validate_market_update(market_update)?;

        match market_update {
            MarketUpdate::Bba { bid, ask } => {
                self.bid = *bid;
                self.ask = *ask;
            }
            MarketUpdate::Trade { .. } => {}
            MarketUpdate::Candle { bid, ask, .. } => {
                self.bid = *bid;
                self.ask = *ask;
            }
        }
        self.current_ts_ns = timestamp_ns as i64;
        self.step += 1;

        Ok(())
    }

    /// Get the mid price
    #[inline]
    pub fn mid_price(&self) -> QuoteCurrency {
        (self.bid + self.ask) / quote!(2)
    }

    /// Get the last observed timestamp in nanoseconts
    #[inline]
    pub fn current_timestamp_ns(&self) -> i64 {
        self.current_ts_ns
    }

    /// Get the last observed bid price.
    #[inline]
    pub fn bid(&self) -> QuoteCurrency {
        self.bid
    }

    /// Get the last observed ask price.
    #[inline]
    pub fn ask(&self) -> QuoteCurrency {
        self.ask
    }

    #[cfg(test)]
    pub fn from_components(
        price_filter: PriceFilter,
        bid: QuoteCurrency,
        ask: QuoteCurrency,
        current_ts_ns: i64,
        step: u64,
    ) -> Self {
        Self {
            price_filter,
            bid,
            ask,
            current_ts_ns,
            step,
        }
    }
}
