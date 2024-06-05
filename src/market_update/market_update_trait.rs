use crate::{
    prelude::{Currency, LimitOrder, MarketState, Pending, PriceFilter},
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
