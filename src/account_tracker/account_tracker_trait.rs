use crate::{
    prelude::{Currency, MarketState, QuoteCurrency, Side, UserBalances},
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
    fn update(&mut self, market_state: &MarketState);

    /// Process information about the user balances.
    fn sample_user_balances(&mut self, user_balances: &UserBalances<M>, mid_price: QuoteCurrency);

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
