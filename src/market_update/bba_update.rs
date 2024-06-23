use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{Currency, LimitOrder, MarketState, Pending, PriceFilter, QuoteCurrency},
    Result,
};

/// An update to the best bid and ask has occured.
/// For now we don't handle the quantity a these price levels.
/// This will change in future versions.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Bba {
    /// The new best bid
    pub bid: QuoteCurrency,
    /// The new best ask
    pub ask: QuoteCurrency,
}

impl<Q, UserOrderId> MarketUpdate<Q, UserOrderId> for Bba
where
    Q: Currency,
    UserOrderId: Clone,
{
    fn limit_order_filled(
        &self,
        _limit_order: &LimitOrder<Q, UserOrderId, Pending<Q>>,
    ) -> Option<Q> {
        None
    }

    fn validate_market_update(&self, price_filter: &PriceFilter) -> Result<()> {
        enforce_min_price(price_filter.min_price(), self.bid)?;
        enforce_min_price(price_filter.min_price(), self.ask)?;
        enforce_max_price(price_filter.max_price(), self.bid)?;
        enforce_max_price(price_filter.max_price(), self.ask)?;
        enforce_step_size(price_filter.tick_size(), self.bid)?;
        enforce_step_size(price_filter.tick_size(), self.ask)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        Ok(())
    }

    fn update_market_state(&self, market_state: &mut MarketState) {
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
