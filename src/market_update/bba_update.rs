use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{Currency, LimitOrder, MarketState, Mon, Pending, PriceFilter, QuoteCurrency},
    types::UserOrderIdT,
    Result,
};

/// An update to the best bid and ask has occured.
/// For now we don't handle the quantity a these price levels.
/// This will change in future versions.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Bba<I, const D: u8>
where
    I: Mon<D>,
{
    /// The new best bid
    pub bid: QuoteCurrency<I, D>,
    /// The new best ask
    pub ask: QuoteCurrency<I, D>,
}

impl<I, const D: u8> std::fmt::Display for Bba<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bid: {}, ask: {}", self.bid, self.ask)
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> MarketUpdate<I, D, BaseOrQuote, UserOrderId>
    for Bba<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderId: UserOrderIdT,
{
    #[inline(always)]
    fn limit_order_filled(
        &self,
        _limit_order: &LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
    ) -> Option<BaseOrQuote> {
        None
    }

    fn validate_market_update(&self, price_filter: &PriceFilter<I, D>) -> Result<()> {
        enforce_min_price(price_filter.min_price(), self.bid)?;
        enforce_min_price(price_filter.min_price(), self.ask)?;
        enforce_max_price(price_filter.max_price(), self.bid)?;
        enforce_max_price(price_filter.max_price(), self.ask)?;
        enforce_step_size(price_filter.tick_size(), self.bid)?;
        enforce_step_size(price_filter.tick_size(), self.ask)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        Ok(())
    }

    #[inline]
    fn update_market_state(&self, market_state: &mut MarketState<I, D>) {
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
