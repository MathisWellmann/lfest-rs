use crate::{
    prelude::{Currency, LimitOrder, MarketState, Mon, Pending, PriceFilter},
    types::UserOrderIdT,
    Result,
};

/// The interface of what a market update must be able to do.
pub trait MarketUpdate<I, const D: u8, BaseOrQuote, UserOrderId>:
    std::fmt::Debug + std::fmt::Display
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderId: UserOrderIdT,
{
    /// Checks if this market update triggered a specific limit order,
    /// and if so, then how much.
    fn limit_order_filled(
        &self,
        limit_order: &LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
    ) -> Option<BaseOrQuote>;

    /// Checks if the market update satisfies the `PriceFilter`.
    fn validate_market_update(&self, price_filter: &PriceFilter<I, D>) -> Result<()>;

    /// Update the `MarketState` with new information.
    fn update_market_state(&self, market_state: &mut MarketState<I, D>);
}
