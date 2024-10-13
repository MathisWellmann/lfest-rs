use crate::{
    prelude::{CurrencyMarker, LimitOrder, MarketState, Mon, Pending, PriceFilter},
    Result,
};

/// The interface of what a market update must be able to do.
pub trait MarketUpdate<I, const DB: u8, const DQ: u8, BaseOrQuote, UserOrderId>:
    std::fmt::Debug
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
    UserOrderId: Clone,
{
    /// Checks if this market update triggered a specific limit order,
    /// and if so, then how much.
    fn limit_order_filled(
        &self,
        limit_order: &LimitOrder<
            I,
            DB,
            DQ,
            BaseOrQuote,
            UserOrderId,
            Pending<I, DB, DQ, BaseOrQuote>,
        >,
    ) -> Option<BaseOrQuote>;

    /// Checks if the market update satisfies the `PriceFilter`.
    fn validate_market_update(
        &self,
        price_filter: &PriceFilter<I, DB, DQ>,
    ) -> Result<(), I, DB, DQ>;

    /// Update the `MarketState` with new information.
    fn update_market_state(&self, market_state: &mut MarketState<I, DB, DQ>);
}
