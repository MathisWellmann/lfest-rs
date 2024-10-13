use crate::{
    prelude::{MarketState, Mon, QuoteCurrency, Side, UserBalances},
    types::MarginCurrencyMarker,
};

/// Something that tracks the performance of the Account.
///
/// # Generics:
/// - `I` is the numeric type,
/// - `DB` is the constant decimal precision of the `BaseCurrency`.
/// - `DQ` is the constant decimal precision of the `QuoteCurrency`.
pub trait AccountTracker<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: MarginCurrencyMarker<I, DB, DQ>,
{
    /// Update with newest market info.
    fn update(&mut self, market_state: &MarketState<I, DB, DQ>);

    /// Process information about the user balances.
    fn sample_user_balances(
        &mut self,
        user_balances: &UserBalances<BaseOrQuote>,
        mid_price: QuoteCurrency<I, DB, DQ>,
    );

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
    fn log_trade(
        &mut self,
        side: Side,
        price: QuoteCurrency<I, DB, DQ>,
        quantity: BaseOrQuote::PairedCurrency,
    );
}
