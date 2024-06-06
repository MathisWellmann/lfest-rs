use crate::{
    prelude::{Currency, MarketState, QuoteCurrency, Side, TimestampNs},
    types::MarginCurrency,
};

/// Something that tracks the performance of the Account.
/// This allows for greated flexibility over using the FullAccountTracker
/// which can easily use more than 10GB of RAM due to storage of tick-by-tick
/// returns
pub trait AccountTracker<M>: Send
where
    M: Currency + MarginCurrency,
{
    /// Update with newest market info.
    fn update(&mut self, timestamp_ns: TimestampNs, market_state: &MarketState);

    /// Log a fee event.
    fn log_fee(&mut self, fee_in_margin: M);

    /// Log a `LimitOrder` submission event.
    fn log_limit_order_submission(&mut self);

    /// Log a `LimitOrder` cancellation event.
    fn log_limit_order_cancellation(&mut self);

    /// Log a `LimitOrder` fill event.
    fn log_limit_order_fill(&mut self);

    /// Log a `MarketOrder` submission event.
    fn log_market_order_submission(&mut self);

    /// Log a market order fill event.
    fn log_market_order_fill(&mut self);

    /// Log a trade
    fn log_trade(&mut self, side: Side, price: QuoteCurrency, quantity: M::PairedCurrency);
}
