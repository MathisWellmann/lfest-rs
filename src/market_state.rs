use const_decimal::Decimal;
use getset::{CopyGetters, Getters, Setters};

use crate::{
    prelude::{CurrencyMarker, MarketUpdate, Mon, PriceFilter, QuoteCurrency},
    types::{Result, TimestampNs},
};

/// Some information regarding the state of the market.
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `DB`: The constant decimal precision of the `BaseCurrency`.
/// - `DQ`: The constant decimal precision of the `QuoteCurrency`.
#[derive(Debug, Default, Clone, Getters, CopyGetters, Setters)]
pub struct MarketState<I, const DB: u8, const DQ: u8>
where
    I: Mon<DB> + Mon<DQ>,
{
    /// The current bid
    #[getset(get_copy = "pub", set = "pub(crate)")]
    bid: QuoteCurrency<I, DB, DQ>,

    /// The current ask
    #[getset(get_copy = "pub", set = "pub(crate)")]
    ask: QuoteCurrency<I, DB, DQ>,

    /// The current timestamp in nanoseconds
    #[getset(get_copy = "pub")]
    current_ts_ns: TimestampNs,

    /// Used for synchronizing orders.
    #[getset(get_copy = "pub")]
    step: u64,
}

impl<I, const DB: u8, const DQ: u8> MarketState<I, DB, DQ>
where
    I: Mon<DB> + Mon<DQ>,
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
        price_filter: &PriceFilter<I, DB, DQ>,
    ) -> Result<(), I, DB, DQ>
    where
        U: MarketUpdate<I, DB, DQ, BaseOrQuote, UserOrderId>,
        BaseOrQuote: CurrencyMarker<I, DB, DQ>,
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
    pub fn mid_price(&self) -> QuoteCurrency<I, DB, DQ> {
        (self.bid + self.ask) / Decimal::try_from_scaled(I::from(2).unwrap(), 0).unwrap()
    }

    /// Get the last observed timestamp in nanoseconts
    #[inline]
    pub fn current_timestamp_ns(&self) -> TimestampNs {
        self.current_ts_ns
    }

    #[cfg(test)]
    pub fn from_components(
        bid: QuoteCurrency<I, DB, DQ>,
        ask: QuoteCurrency<I, DB, DQ>,
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
