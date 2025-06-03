use const_decimal::Decimal;

use super::MarketUpdate;
use crate::{
    Result,
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{Currency, LimitOrder, MarketState, Mon, Pending, PriceFilter, QuoteCurrency},
    types::{TimestampNs, UserOrderId},
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
    /// The nanosecond timestamp at which this event occurred at the exchange.
    pub timestamp_exchange_ns: TimestampNs,
}

impl<I: Mon<D>, const D: u8> Bba<I, D> {
    /// The mid price between the bid and ask.
    #[inline(always)]
    pub fn mid_price(&self) -> QuoteCurrency<I, D> {
        (self.bid + self.ask) / Decimal::TWO
    }
}

impl<I, const D: u8> std::fmt::Display for Bba<I, D>
where
    I: Mon<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bid: {}, ask: {}, ts: {}",
            self.bid, self.ask, self.timestamp_exchange_ns
        )
    }
}

impl<I, const D: u8, BaseOrQuote> MarketUpdate<I, D, BaseOrQuote> for Bba<I, D>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const CAN_FILL_LIMIT_ORDERS: bool = false;

    #[inline(always)]
    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &self,
        _limit_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Option<BaseOrQuote> {
        unreachable!(
            "This should never be called, because a best bid and ask update can never fill a limit order."
        );
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

    #[inline(always)]
    fn timestamp_exchange_ns(&self) -> TimestampNs {
        self.timestamp_exchange_ns
    }

    #[inline(always)]
    fn can_fill_bids(&self) -> bool {
        false
    }

    #[inline(always)]
    fn can_fill_asks(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BaseCurrency;

    #[test]
    fn size_of_bba() {
        assert_eq!(std::mem::size_of::<Bba<i32, 4>>(), 16);
        assert_eq!(std::mem::size_of::<Bba<i64, 4>>(), 24);
    }

    #[test]
    fn bba_update() {
        let bba = Bba {
            bid: QuoteCurrency::<i64, 1>::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        };
        assert_eq!(bba.mid_price(), QuoteCurrency::new(1005, 1));
        assert_eq!(
            <Bba<i64, 1> as MarketUpdate<i64, 1, BaseCurrency<i64, 1>>>::can_fill_bids(&bba),
            false
        );
        assert_eq!(
            <Bba<i64, 1> as MarketUpdate<i64, 1, BaseCurrency<i64, 1>>>::can_fill_asks(&bba),
            false
        );
    }

    #[test]
    fn bba_update_display() {
        let update = Bba {
            bid: QuoteCurrency::<i64, 1>::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        };
        assert_eq!(
            &update.to_string(),
            "bid: 100.0 Quote, ask: 101.0 Quote, ts: 1"
        );
    }
}
