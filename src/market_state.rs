use const_decimal::Decimal;
use getset::{CopyGetters, Getters, Setters};

use crate::{
    prelude::{Currency, MarketUpdate, Mon, PriceFilter, QuoteCurrency},
    types::{Result, TimestampNs},
};

/// Some information regarding the state of the market.
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currency.
#[derive(Debug, Default, Clone, Getters, CopyGetters, Setters)]
pub struct MarketState<I, const D: u8>
where
    I: Mon<D>,
{
    /// The current bid
    #[getset(get_copy = "pub", set = "pub(crate)")]
    bid: QuoteCurrency<I, D>,

    /// The current ask
    #[getset(get_copy = "pub", set = "pub(crate)")]
    ask: QuoteCurrency<I, D>,

    /// The current timestamp in nanoseconds
    #[getset(get_copy = "pub")]
    current_ts_ns: TimestampNs,

    /// Used for synchronizing orders.
    #[getset(get_copy = "pub")]
    step: u64,
}

impl<I, const D: u8> MarketState<I, D>
where
    I: Mon<D>,
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
        price_filter: &PriceFilter<I, D>,
    ) -> Result<(), I, D>
    where
        U: MarketUpdate<I, D, BaseOrQuote, UserOrderId>,
        BaseOrQuote: Currency<I, D>,
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
    pub fn mid_price(&self) -> QuoteCurrency<I, D> {
        (self.bid + self.ask) / Decimal::try_from_scaled(I::from(2).unwrap(), 0).unwrap()
    }

    /// Get the last observed timestamp in nanoseconts
    #[inline]
    pub fn current_timestamp_ns(&self) -> TimestampNs {
        self.current_ts_ns
    }

    #[cfg(test)]
    pub fn from_components(
        bid: QuoteCurrency<I, D>,
        ask: QuoteCurrency<I, D>,
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
