use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{Currency, LimitOrder, MarketState, Pending, PriceFilter, QuoteCurrency, Side},
    Result,
};

/// A new candle has been created.
/// Here we can use the `high` and `low` prices to see if our simulated resting orders
/// have been executed over the last period as a proxy in absence of actual `Trade` flow.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Candle {
    /// The best bid at the time of candle creation
    pub bid: QuoteCurrency,
    /// The best ask at the time of candle creation
    pub ask: QuoteCurrency,
    /// The low price of the candle
    pub low: QuoteCurrency,
    /// The high price of the candle
    pub high: QuoteCurrency,
}

impl<Q, UserOrderId> MarketUpdate<Q, UserOrderId> for Candle
where
    Q: Currency,
    UserOrderId: Clone,
{
    fn limit_order_filled(&self, order: &LimitOrder<Q, UserOrderId, Pending<Q>>) -> Option<Q> {
        // As a simplifying assumption, the order always get executed fully when using candles if the price is right.
        if match order.side() {
            Side::Buy => self.low < order.limit_price(),
            Side::Sell => self.high > order.limit_price(),
        } {
            // Order is executed fully with candles.
            Some(match order.side() {
                Side::Buy => order.remaining_quantity(),
                Side::Sell => order.remaining_quantity().into_negative(),
            })
        } else {
            None
        }
    }

    fn validate_market_update(&self, price_filter: &PriceFilter) -> Result<()> {
        enforce_min_price(price_filter.min_price, self.bid)?;
        enforce_min_price(price_filter.min_price, self.ask)?;
        enforce_min_price(price_filter.min_price, self.low)?;
        enforce_min_price(price_filter.min_price, self.high)?;
        enforce_max_price(price_filter.max_price, self.bid)?;
        enforce_max_price(price_filter.max_price, self.ask)?;
        enforce_max_price(price_filter.max_price, self.low)?;
        enforce_max_price(price_filter.max_price, self.high)?;
        enforce_step_size(price_filter.tick_size, self.bid)?;
        enforce_step_size(price_filter.tick_size, self.ask)?;
        enforce_step_size(price_filter.tick_size, self.low)?;
        enforce_step_size(price_filter.tick_size, self.high)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        enforce_bid_ask_spread(self.low, self.high)?;
        Ok(())
    }

    fn update_market_state(&self, market_state: &mut MarketState) {
        market_state.set_bid(self.bid);
        market_state.set_ask(self.ask);
    }
}

/// Creates the `Candle` struct used as a `MarketUpdate`.
#[macro_export]
macro_rules! candle {
    ( $b:expr, $a:expr, $l:expr, $h:expr ) => {{
        $crate::prelude::Candle {
            bid: $b,
            ask: $a,
            low: $l,
            high: $h,
        }
    }};
}
