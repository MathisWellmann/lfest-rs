use crate::{
    prelude::{CurrencyMarker, LimitOrder, MarketState, Mon, Monies, Pending, PriceFilter},
    Result,
};

/// The interface of what a market update must be able to do.
pub trait MarketUpdate<T, BaseOrQuote, UserOrderId>: std::fmt::Debug
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    UserOrderId: Clone,
{
    /// Checks if this market update triggered a specific limit order,
    /// and if so, then how much.
    fn limit_order_filled(
        &self,
        limit_order: &LimitOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
    ) -> Option<Monies<T, BaseOrQuote>>;

    /// Checks if the market update satisfies the `PriceFilter`.
    fn validate_market_update(&self, price_filter: &PriceFilter<T>) -> Result<(), T>;

    /// Update the `MarketState` with new information.
    fn update_market_state(&self, market_state: &mut MarketState<T>);
}
