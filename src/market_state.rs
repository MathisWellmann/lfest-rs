use getset::{CopyGetters, Getters, Setters};

use crate::{
    prelude::{CurrencyMarker, MarketUpdate, Mon, Monies, PriceFilter, Quote},
    types::{Result, TimestampNs},
};

/// Some information regarding the state of the market.
#[derive(Debug, Default, Clone, Getters, CopyGetters, Setters)]
pub struct MarketState<T: Mon> {
    /// The current bid
    #[getset(get_copy = "pub", set = "pub(crate)")]
    bid: Monies<T, Quote>,

    /// The current ask
    #[getset(get_copy = "pub", set = "pub(crate)")]
    ask: Monies<T, Quote>,

    /// The current timestamp in nanoseconds
    #[getset(get_copy = "pub")]
    current_ts_ns: TimestampNs,

    /// Used for synchronizing orders.
    #[getset(get_copy = "pub")]
    step: u64,
}

impl<T> MarketState<T>
where
    T: Mon,
{
    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    pub(crate) fn update_state<U, BaseOrQuote, UserOrderId>(
        &mut self,
        timestamp_ns: TimestampNs,
        market_update: &U,
        price_filter: &PriceFilter<T>,
    ) -> Result<(), T>
    where
        U: MarketUpdate<T, BaseOrQuote, UserOrderId>,
        BaseOrQuote: CurrencyMarker<T>,
        UserOrderId: Clone,
    {
        market_update.validate_market_update(price_filter)?;
        market_update.update_market_state(self);

        self.current_ts_ns = timestamp_ns;
        self.step += 1;

        Ok(())
    }

    /// Get the mid price
    #[inline]
    pub fn mid_price(&self) -> Monies<T, Quote> {
        (self.bid + self.ask) / Monies::new(T::from(2_u8))
    }

    /// Get the last observed timestamp in nanoseconts
    #[inline]
    pub fn current_timestamp_ns(&self) -> TimestampNs {
        self.current_ts_ns
    }

    #[cfg(test)]
    pub fn from_components(
        bid: Monies<T, Quote>,
        ask: Monies<T, Quote>,
        current_ts_ns: TimestampNs,
        step: u64,
    ) -> Self {
        Self {
            bid,
            ask,
            current_ts_ns,
            step,
        }
    }
}
