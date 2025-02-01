use crate::{
    prelude::{Currency, LimitOrder, MarketState, Mon, Pending, PriceFilter},
    types::{TimestampNs, UserOrderId},
    Result,
};

/// The interface of what a market update must be able to do.
pub trait MarketUpdate<I, const D: u8, BaseOrQuote>: std::fmt::Debug + std::fmt::Display
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// performance optimization to speed up hot-path, when we don't need to check limit order fills for `Bba` updates.
    const CAN_FILL_LIMIT_ORDERS: bool;

    /// Checks if this market update triggered a specific limit order,
    /// and if so, then how much.
    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &self,
        limit_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Option<BaseOrQuote>;

    /// Checks if the market update satisfies the `PriceFilter`.
    fn validate_market_update(&self, price_filter: &PriceFilter<I, D>) -> Result<()>;

    /// Update the `MarketState` with new information.
    fn update_market_state(&self, market_state: &mut MarketState<I, D>);

    /// The nanosecond timestamp when the market update occurred at the exchange.
    fn timestamp_exchange_ns(&self) -> TimestampNs;
}
