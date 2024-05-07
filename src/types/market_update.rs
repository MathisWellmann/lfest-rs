use super::{Currency, LimitOrder, Pending, Side};
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{MarketState, PriceFilter},
    types::QuoteCurrency,
    utils::min,
    Result,
};

/// The interface of what a market update must be able to do.
pub trait MarketUpdate<Q, UserOrderId>
where
    Q: Currency,
    UserOrderId: Clone,
{
    /// Checks if this market update triggered a specific limit order,
    /// and if so, then how much.
    fn limit_order_filled(&self, limit_order: &LimitOrder<Q, UserOrderId, Pending<Q>>)
        -> Option<Q>;

    /// Checks if the market update satisfies the `PriceFilter`.
    fn validate_market_update(&self, price_filter: &PriceFilter) -> Result<()>;

    /// Update the `MarketState` with new information.
    fn update_market_state(&self, market_state: &mut MarketState);
}

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
        enforce_min_price(price_filter.min_price, self.bid)?;
        enforce_min_price(price_filter.min_price, self.ask)?;
        enforce_max_price(price_filter.max_price, self.bid)?;
        enforce_max_price(price_filter.max_price, self.ask)?;
        enforce_step_size(price_filter.tick_size, self.bid)?;
        enforce_step_size(price_filter.tick_size, self.ask)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        Ok(())
    }

    fn update_market_state(&self, market_state: &mut MarketState) {
        market_state.set_bid(self.bid);
        market_state.set_ask(self.ask);
    }
}

/// A taker trade that consumes liquidity in the book.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Trade<Q> {
    /// The price at which the trade executed at.
    pub price: QuoteCurrency,
    /// The executed quantity.
    /// Generic denotation, e.g either Quote or Base currency denoted.
    pub quantity: Q,
    /// Either a buy or sell order.
    pub side: Side,
}

impl<Q, UserOrderId> MarketUpdate<Q, UserOrderId> for Trade<Q>
where
    Q: Currency,
    UserOrderId: Clone,
{
    fn limit_order_filled(&self, order: &LimitOrder<Q, UserOrderId, Pending<Q>>) -> Option<Q> {
        if match order.side() {
            Side::Buy => self.price <= order.limit_price() && matches!(self.side, Side::Sell),
            Side::Sell => self.price >= order.limit_price() && matches!(self.side, Side::Buy),
        } {
            // Execute up to the quantity of the incoming `Trade`.
            let filled_qty = min(self.quantity, order.quantity());
            Some(filled_qty)
        } else {
            None
        }
    }

    fn validate_market_update(&self, price_filter: &PriceFilter) -> Result<()> {
        enforce_min_price(price_filter.min_price, self.price)?;
        enforce_max_price(price_filter.max_price, self.price)?;
        enforce_step_size(price_filter.tick_size, self.price)?;
        Ok(())
    }

    fn update_market_state(&self, _market_state: &mut MarketState) {}
}

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
                Side::Buy => order.quantity(),
                Side::Sell => order.quantity().into_negative(),
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

/// Creates the `Bba` struct used as a `MarketUpdate`.
#[macro_export]
macro_rules! bba {
    ( $b:expr, $a:expr ) => {{
        $crate::prelude::Bba { bid: $b, ask: $a }
    }};
}

/// Creates the `Trade` struct used as a `MarketUpdate`.
#[macro_export]
macro_rules! trade {
    ( $price:expr, $quantity:expr, $side:expr ) => {{
        $crate::prelude::Trade {
            price: $price,
            quantity: $quantity,
            side: $side,
        }
    }};
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
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn bba_macro() {
        let m = bba!(quote!(100.0), quote!(100.1));

        assert_eq!(
            m,
            Bba {
                bid: quote!(100.0),
                ask: quote!(100.1)
            }
        );
    }

    #[test]
    fn candle_macro() {
        let c = candle!(quote!(100.0), quote!(100.1), quote!(100.0), quote!(100.1));

        assert_eq!(
            c,
            Candle {
                bid: quote!(100.0),
                ask: quote!(100.1),
                low: quote!(100.0),
                high: quote!(100.1),
            }
        )
    }
}
