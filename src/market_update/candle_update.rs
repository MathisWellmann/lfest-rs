use getset::CopyGetters;
use typed_builder::TypedBuilder;

use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{Currency, LimitOrder, MarketState, Mon, Pending, PriceFilter, QuoteCurrency, Side},
    types::{TimestampNs, UserOrderId},
    Result,
};

/// A new candle has been created.
/// Here we can use the `high` and `low` prices to see if our simulated resting orders
/// have been executed over the last period as a proxy in absence of actual `Trade` flow.
#[derive(Debug, Clone, Copy, Eq, PartialEq, TypedBuilder, CopyGetters)]
pub struct Candle<I, const D: u8>
where
    I: Mon<D>,
{
    /// The best bid at the time of candle creation
    #[getset(get_copy = "pub")]
    bid: QuoteCurrency<I, D>,

    /// The best ask at the time of candle creation
    #[getset(get_copy = "pub")]
    ask: QuoteCurrency<I, D>,

    /// The low price of the candle
    #[getset(get_copy = "pub")]
    low: QuoteCurrency<I, D>,

    /// The high price of the candle
    #[getset(get_copy = "pub")]
    high: QuoteCurrency<I, D>,

    /// The nanosecond timestamp at which this event occurred at the exchange.
    #[getset(get_copy = "pub")]
    timestamp_exchange_ns: TimestampNs,
}

impl<I, const D: u8> std::fmt::Display for Candle<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bid: {}, ask: {}, high: {}, low: {}",
            self.bid, self.ask, self.high, self.low
        )
    }
}

impl<I, const D: u8, BaseOrQuote> MarketUpdate<I, D, BaseOrQuote> for Candle<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const CAN_FILL_LIMIT_ORDERS: bool = true;

    #[inline]
    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Option<BaseOrQuote> {
        debug_assert!(order.remaining_quantity() > BaseOrQuote::zero());

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

    fn validate_market_update(&self, price_filter: &PriceFilter<I, D>) -> Result<()> {
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

    #[inline]
    fn update_market_state(&self, market_state: &mut MarketState<I, D>) {
        market_state.set_bid(self.bid);
        market_state.set_ask(self.ask);
    }

    #[inline(always)]
    fn timestamp_exchange_ns(&self) -> TimestampNs {
        self.timestamp_exchange_ns
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::{BaseCurrency, ExchangeOrderMeta};

    #[test]
    fn candle_update() {
        let candle = Candle {
            bid: QuoteCurrency::<i64, 5>::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            low: QuoteCurrency::new(95, 0),
            high: QuoteCurrency::new(105, 0),
            timestamp_exchange_ns: 1.into(),
        };
        let new_order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(94, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let order = new_order.into_pending(meta);

        let price_filter = PriceFilter::default();
        <Candle<_, 5> as MarketUpdate<_, 5, BaseCurrency<_, 5>>>::validate_market_update(
            &candle,
            &price_filter,
        )
        .unwrap();
        assert_eq!(candle.limit_order_filled(&order), None);
        assert_eq!(candle.timestamp_exchange_ns(), 1.into());
    }
}
