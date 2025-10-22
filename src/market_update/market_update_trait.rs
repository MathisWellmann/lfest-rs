use crate::{
    Result,
    prelude::{
        Currency,
        LimitOrder,
        MarketState,
        Mon,
        Pending,
        PriceFilter,
    },
    types::{
        TimestampNs,
        UserOrderId,
    },
};

/// If `true`, the `MarketUpdate` can no longer fill limit orders.
pub(crate) type Exhausted = bool;

/// The interface of what a market update must be able to do.
pub trait MarketUpdate<I, const D: u8, BaseOrQuote>:
    Clone + std::fmt::Debug + std::fmt::Display
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// performance optimization to speed up hot-path, when we don't need to check limit order fills for `Bba` updates.
    const CAN_FILL_LIMIT_ORDERS: bool;

    /// If `true`, the `MarketUpdate` can fill bids.
    fn can_fill_bids(&self) -> bool;

    /// If `true`, the `MarketUpdate` can fill asks.
    fn can_fill_asks(&self) -> bool;

    /// Checks if this market update fills a limit order,
    /// If it fills the limit order (even partially), its state is mutate to reflect the liquidity difference.
    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &mut self,
        limit_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Option<(BaseOrQuote, Exhausted)>;

    /// Checks if the market update satisfies the `PriceFilter`.
    fn validate_market_update(&self, price_filter: &PriceFilter<I, D>) -> Result<()>;

    /// Update the `MarketState` with new information.
    fn update_market_state(&self, market_state: &mut MarketState<I, D>);

    /// The nanosecond timestamp when the market update occurred at the exchange.
    fn timestamp_exchange_ns(&self) -> TimestampNs;
}
