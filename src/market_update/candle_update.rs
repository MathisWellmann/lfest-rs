use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{
        CurrencyMarker, LimitOrder, MarketState, Mon, Pending, PriceFilter, QuoteCurrency, Side,
    },
    Result,
};

/// A new candle has been created.
/// Here we can use the `high` and `low` prices to see if our simulated resting orders
/// have been executed over the last period as a proxy in absence of actual `Trade` flow.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Candle<I, const DB: u8, const DQ: u8>
where
    I: Mon<DB> + Mon<DQ>,
{
    /// The best bid at the time of candle creation
    pub bid: QuoteCurrency<I, DB, DQ>,
    /// The best ask at the time of candle creation
    pub ask: QuoteCurrency<I, DB, DQ>,
    /// The low price of the candle
    pub low: QuoteCurrency<I, DB, DQ>,
    /// The high price of the candle
    pub high: QuoteCurrency<I, DB, DQ>,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote, UserOrderId>
    MarketUpdate<I, DB, DQ, BaseOrQuote, UserOrderId> for Candle<I, DB, DQ>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
    UserOrderId: Clone,
{
    fn limit_order_filled(
        &self,
        order: &LimitOrder<I, DB, DQ, BaseOrQuote, UserOrderId, Pending<I, DB, DQ, BaseOrQuote>>,
    ) -> Option<BaseOrQuote> {
        assert!(order.remaining_quantity() > BaseOrQuote::zero());

        // As a simplifying assumption, the order always get executed fully when using candles if the price is right.
        if match order.side() {
            Side::Buy => self.low < order.limit_price(),
            Side::Sell => self.high > order.limit_price(),
        } {
            // Order is executed fully with candles.
            Some(match order.side() {
                Side::Buy => order.remaining_quantity(),
                Side::Sell => order.remaining_quantity(),
            })
        } else {
            None
        }
    }

    fn validate_market_update(
        &self,
        price_filter: &PriceFilter<I, DB, DQ>,
    ) -> Result<(), I, DB, DQ> {
        enforce_min_price(price_filter.min_price(), self.bid)?;
        enforce_min_price(price_filter.min_price(), self.ask)?;
        enforce_min_price(price_filter.min_price(), self.low)?;
        enforce_min_price(price_filter.min_price(), self.high)?;
        enforce_max_price(price_filter.max_price(), self.bid)?;
        enforce_max_price(price_filter.max_price(), self.ask)?;
        enforce_max_price(price_filter.max_price(), self.low)?;
        enforce_max_price(price_filter.max_price(), self.high)?;
        enforce_step_size(price_filter.tick_size(), self.bid)?;
        enforce_step_size(price_filter.tick_size(), self.ask)?;
        enforce_step_size(price_filter.tick_size(), self.low)?;
        enforce_step_size(price_filter.tick_size(), self.high)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        enforce_bid_ask_spread(self.low, self.high)?;
        Ok(())
    }

    fn update_market_state(&self, market_state: &mut MarketState<I, DB, DQ>) {
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
