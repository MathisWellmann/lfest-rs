use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{CurrencyMarker, LimitOrder, MarketState, Mon, Monies, Pending, PriceFilter, Quote},
    Result,
};

/// An update to the best bid and ask has occured.
/// For now we don't handle the quantity a these price levels.
/// This will change in future versions.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Bba<T>
where
    T: Mon,
{
    /// The new best bid
    pub bid: Monies<T, Quote>,
    /// The new best ask
    pub ask: Monies<T, Quote>,
}

impl<T, BaseOrQuote, UserOrderId> MarketUpdate<T, BaseOrQuote, UserOrderId> for Bba<T>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    UserOrderId: Clone,
{
    fn limit_order_filled(
        &self,
        _limit_order: &LimitOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
    ) -> Option<Monies<T, BaseOrQuote>> {
        None
    }

    fn validate_market_update(&self, price_filter: &PriceFilter<T>) -> Result<(), T> {
        enforce_min_price(price_filter.min_price(), self.bid)?;
        enforce_min_price(price_filter.min_price(), self.ask)?;
        enforce_max_price(price_filter.max_price(), self.bid)?;
        enforce_max_price(price_filter.max_price(), self.ask)?;
        enforce_step_size(price_filter.tick_size(), self.bid)?;
        enforce_step_size(price_filter.tick_size(), self.ask)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        Ok(())
    }

    fn update_market_state(&self, market_state: &mut MarketState<T>) {
        market_state.set_bid(self.bid);
        market_state.set_ask(self.ask);
    }
}

/// Creates the `Bba` struct used as a `MarketUpdate`.
#[macro_export]
macro_rules! bba {
    ( $b:expr, $a:expr ) => {{
        $crate::prelude::Bba { bid: $b, ask: $a }
    }};
}
