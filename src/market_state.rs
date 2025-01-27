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

    /// The last trade price.
    #[getset(get_copy = "pub", set = "pub(crate)")]
    last_trade_price: QuoteCurrency<I, D>,

    /// The current timestamp in nanoseconds
    #[getset(get_copy = "pub")]
    current_ts_ns: TimestampNs,

    /// Used for synchronizing orders.
    #[getset(get_copy = "pub")]
    step: u64,
}

impl<I: Mon<D>, const D: u8> std::fmt::Display for MarketState<I, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bid: {}, ask: {}, ts_ns: {}, step: {}",
            self.bid, self.ask, self.current_ts_ns, self.step
        )
    }
}

impl<I, const D: u8> MarketState<I, D>
where
    I: Mon<D>,
{
    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `market_update`: Newest market information
    /// `price_filter`: The pricing rules.
    ///
    pub(crate) fn update_state<U, BaseOrQuote>(
        &mut self,
        market_update: &U,
        price_filter: &PriceFilter<I, D>,
    ) -> Result<()>
    where
        U: MarketUpdate<I, D, BaseOrQuote>,
        BaseOrQuote: Currency<I, D>,
    {
        // Only in debug mode do we care to validate the market update, because usually the update comes from an exchange source.
        debug_assert!(market_update.validate_market_update(price_filter).is_ok());
        market_update.update_market_state(self);

        self.current_ts_ns = market_update.timestamp_exchange_ns();
        self.step += 1;

        Ok(())
    }

    /// Get the mid price
    #[inline(always)]
    pub fn mid_price(&self) -> QuoteCurrency<I, D> {
        (self.bid + self.ask) / Decimal::TWO
    }

    /// Get the last observed timestamp in nanoseconts
    #[inline(always)]
    pub fn current_timestamp_ns(&self) -> TimestampNs {
        self.current_ts_ns
    }

    #[cfg(test)]
    pub fn from_components(
        bid: QuoteCurrency<I, D>,
        ask: QuoteCurrency<I, D>,
        last_trade_price: QuoteCurrency<I, D>,
        current_ts_ns: TimestampNs,
        step: u64,
    ) -> Self {
        Self {
            bid,
            ask,
            last_trade_price,
            current_ts_ns,
            step,
        }
    }
}
