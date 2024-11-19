use crate::{
    prelude::{MarketState, Mon, QuoteCurrency, Side, UserBalances},
    types::{LimitOrder, MarginCurrency, MarketOrder, NewOrder, UserOrderIdT},
};

/// Something that tracks the performance of the Account.
///
/// # Generics:
/// - `I` is the numeric type,
/// - `D` is the constant decimal precision of the currencies
pub trait AccountTracker<I, const D: u8, BaseOrQuote, UserOrderId>: std::fmt::Debug
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
    UserOrderId: UserOrderIdT,
{
    /// Update with newest market info.
    fn update(&mut self, market_state: &MarketState<I, D>);

    /// Process information about the user balances.
    fn sample_user_balances(
        &mut self,
        user_balances: &UserBalances<I, D, BaseOrQuote>,
        mid_price: QuoteCurrency<I, D>,
    );

    /// Log a `LimitOrder` submission event.
    fn log_limit_order_submission(
        &mut self,
        limit_order: &LimitOrder<I, D, BaseOrQuote::PairedCurrency, UserOrderId, NewOrder>,
    );

    /// Log a `LimitOrder` cancellation event.
    fn log_limit_order_cancellation(&mut self);

    /// Log a `LimitOrder` fill event.
    fn log_limit_order_fill(&mut self, fully_filled: bool, fill_qty: BaseOrQuote::PairedCurrency);

    /// Log a `MarketOrder` submission event.
    fn log_market_order_submission(
        &mut self,
        market_order: &MarketOrder<I, D, BaseOrQuote::PairedCurrency, UserOrderId, NewOrder>,
    );

    /// Log a market order fill event.
    fn log_market_order_fill(&mut self);

    /// Log a trade
    fn log_trade(
        &mut self,
        side: Side,
        price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote::PairedCurrency,
    );
}
