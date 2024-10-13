use super::MarketUpdate;
use crate::{
    order_filters::{
        enforce_bid_ask_spread, enforce_max_price, enforce_min_price, enforce_step_size,
    },
    prelude::{CurrencyMarker, LimitOrder, MarketState, Mon, Pending, PriceFilter, QuoteCurrency},
    Result,
};

/// An update to the best bid and ask has occured.
/// For now we don't handle the quantity a these price levels.
/// This will change in future versions.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Bba<I, const DB: u8, const DQ: u8>
where
    I: Mon<DB> + Mon<DQ>,
{
    /// The new best bid
    pub bid: QuoteCurrency<I, DB, DQ>,
    /// The new best ask
    pub ask: QuoteCurrency<I, DB, DQ>,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote, UserOrderId>
    MarketUpdate<I, DB, DQ, BaseOrQuote, UserOrderId> for Bba<I, DB, DQ>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
    UserOrderId: Clone,
{
    fn limit_order_filled(
        &self,
        _limit_order: &LimitOrder<
            I,
            DB,
            DQ,
            BaseOrQuote,
            UserOrderId,
            Pending<I, DB, DQ, BaseOrQuote>,
        >,
    ) -> Option<BaseOrQuote> {
        None
    }

    fn validate_market_update(
        &self,
        price_filter: &PriceFilter<I, DB, DQ>,
    ) -> Result<(), I, DB, DQ> {
        enforce_min_price(price_filter.min_price(), self.bid)?;
        enforce_min_price(price_filter.min_price(), self.ask)?;
        enforce_max_price(price_filter.max_price(), self.bid)?;
        enforce_max_price(price_filter.max_price(), self.ask)?;
        enforce_step_size(price_filter.tick_size(), self.bid)?;
        enforce_step_size(price_filter.tick_size(), self.ask)?;
        enforce_bid_ask_spread(self.bid, self.ask)?;
        Ok(())
    }

    fn update_market_state(&self, market_state: &mut MarketState<I, DB, DQ>) {
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
