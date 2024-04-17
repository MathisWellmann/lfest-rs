use crate::{
    prelude::{Account, Currency, MarketState, QuoteCurrency, Side},
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
    /// Update with each tick, using data provided in update_state method of
    /// Exchange.
    ///
    /// # Arguments:
    /// `timestamp_ns`: timestamp of latest tick in nanoseconds
    /// `price`: price of latest tick
    /// `upnl`: unrealized profit and loss of account in current tick
    fn update(&mut self, timestamp_ns: u64, market_state: &MarketState, account: &Account<M>);

    /// Log a realized profit and loss event
    ///
    /// # Arguments:
    /// `net_rpnl`: The realized profit and loss, denoted in margin currency.
    /// `ts_ns`: The timestamp in nanoseconds of this event.
    fn log_rpnl(&mut self, net_rpnl: M, ts_ns: i64);

    /// Log a fee, measured in the margin currency
    fn log_fee(&mut self, fee_in_margin: M);

    /// Log a limit order submission event
    fn log_limit_order_submission(&mut self);

    /// Log a limit order cancellation event
    fn log_limit_order_cancellation(&mut self);

    /// Log a limit order fill event.
    fn log_limit_order_fill(&mut self);

    /// Log a market order fill event.
    fn log_market_order_fill(&mut self);

    /// Log a trade event where some order got filled and the position changed
    fn log_trade(&mut self, side: Side, price: QuoteCurrency, quantity: M::PairedCurrency);
}
