use std::fmt::Display;

use getset::CopyGetters;
use sliding_features::{
    pure_functions::Echo,
    rolling::{Drawdown, LnReturn, WelfordRolling},
    View,
};

use crate::{
    account_tracker::AccountTracker,
    prelude::{Side, UserBalances},
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency, TimestampNs},
    utils::{balance_sum, decimal_to_f64},
};

const DAILY_NS: i64 = 86_400_000_000_000;

/// Keep track of Account performance statistics.
/// Must update in `O(1)` and also compute performance measures in `O(1)`.
#[derive(Debug, Clone, CopyGetters)]
pub struct FullAccountTracker<M>
where
    M: MarginCurrency,
{
    /// Wallet balance at the start.
    #[getset(get_copy = "pub")]
    wallet_balance_start: M,

    /// The number of submitted limit orders.
    #[getset(get_copy = "pub")]
    num_submitted_limit_orders: usize,
    /// The number of cancelled limit orders.
    #[getset(get_copy = "pub")]
    num_cancelled_limit_orders: usize,
    /// The number of fully filled limit orders. Partial fills not included.
    #[getset(get_copy = "pub")]
    num_fully_filled_limit_orders: usize,

    /// The number of submitted market orders.
    #[getset(get_copy = "pub")]
    num_submitted_market_orders: usize,
    /// The number of filled_market_orders.
    #[getset(get_copy = "pub")]
    num_filled_market_orders: usize,

    /// The total volume bought.
    #[getset(get_copy = "pub")]
    buy_volume: M,

    /// The total volume sold.
    #[getset(get_copy = "pub")]
    sell_volume: M,

    /// The cumulative fees paid.
    #[getset(get_copy = "pub")]
    cumulative_fees: M,

    price_first: QuoteCurrency,
    price_last: QuoteCurrency,
    ts_first: TimestampNs,
    ts_last: TimestampNs,

    /// Keep track of natural logarithmic returns of users funds.
    user_balances_ln_return: LnReturn<Echo>,
    drawdown_user_balances: Drawdown<Echo>, // Drawdown of realized user balances.
    drawdown_market: Drawdown<Echo>,        // Drawdown of the market.
    user_balances_ln_return_stats: WelfordRolling<Echo>, // Used for `sharpe` and `kelly_leverage`
    user_balances_pos_ln_return_stats: WelfordRolling<Echo>, // Used for `sortino`

    /// last sum of all user balances.
    last_balance_sum: M,
}

/// TODO: create its own `risk` crate out of these implementations for better
/// reusability and testability
impl<M> FullAccountTracker<M>
where
    M: Currency + MarginCurrency + Send,
{
    /// Create a new instance of `Self`.
    #[must_use]
    pub fn new(starting_wb: M) -> Self {
        assert!(
            starting_wb > M::new_zero(),
            "The starting wallet balance must be greater than zero"
        );

        FullAccountTracker {
            wallet_balance_start: starting_wb,

            num_submitted_limit_orders: 0,
            num_cancelled_limit_orders: 0,
            num_fully_filled_limit_orders: 0,
            num_submitted_market_orders: 0,
            num_filled_market_orders: 0,

            buy_volume: M::new_zero(),
            sell_volume: M::new_zero(),

            cumulative_fees: M::new_zero(),
            price_first: quote!(0.0),
            price_last: quote!(0.0),
            ts_first: TimestampNs::from(0),
            ts_last: TimestampNs::from(0),

            user_balances_ln_return: LnReturn::default(),
            drawdown_user_balances: Drawdown::default(),
            drawdown_market: Drawdown::default(),
            user_balances_ln_return_stats: WelfordRolling::default(),
            user_balances_pos_ln_return_stats: WelfordRolling::default(),

            last_balance_sum: M::new_zero(),
        }
    }

    /// Would be the return of buy and hold strategy
    pub fn buy_and_hold_return(&self) -> M {
        let qty = self.wallet_balance_start.convert(self.price_first);
        M::pnl(self.price_first, self.price_last, qty)
    }

    /// Would be the return of sell and hold strategy
    pub fn sell_and_hold_return(&self) -> M {
        self.buy_and_hold_return().into_negative()
    }

    /// Return the number of trading days
    pub fn num_trading_days(&self) -> i64 {
        assert!(
            self.ts_last >= self.ts_first,
            "Last timestamp must be after first."
        );

        Into::<i64>::into((self.ts_last - self.ts_first) / TimestampNs::from(DAILY_NS))
    }

    /// Return the ratio of filled limit orders vs number of submitted limit
    /// orders
    pub fn limit_order_fill_ratio(&self) -> f64 {
        self.num_fully_filled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

    /// Return the ratio of limit order cancellations vs number of submitted
    /// limit orders
    pub fn limit_order_cancellation_ratio(&self) -> f64 {
        self.num_cancelled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

    /// The total volume traded.
    pub fn turnover(&self) -> M {
        self.buy_volume + self.sell_volume
    }

    /// The drawdown of user balances.
    pub fn drawdown_user_balances(&self) -> f64 {
        self.drawdown_user_balances.last().unwrap_or(0.0)
    }

    /// The drawdown the market experienced.
    pub fn drawdown_market(&self) -> f64 {
        self.drawdown_market.last().unwrap_or(0.0)
    }

    /// The realized profit and loss of the users account.
    /// Unrealized pnl not included.
    pub fn rpnl(&self) -> M {
        self.last_balance_sum - self.wallet_balance_start
    }

    /// Return the raw sharpe ratio that has been derived from the sampled returns of the users balances.
    /// This sharpe ratio is not annualized and does not include a risk free rate.
    pub fn sharpe(&self) -> Option<f64> {
        let std_dev = self.user_balances_ln_return_stats.last()?;
        let mean_return = self.user_balances_ln_return_stats.mean();

        // No risk free rate subtracted.
        Some(mean_return / std_dev)
    }

    /// Returns the theoretical kelly leverage that would maximize the compounded growth rate,
    /// assuming the returns are normally distributed. Which they almost never are. So be aware.
    pub fn kelly_leverage(&self) -> f64 {
        let mean_return = self.user_balances_ln_return_stats.mean();
        let return_variance = self.user_balances_ln_return_stats.variance();
        assert!(return_variance >= 0.0);

        if return_variance == 0.0 {
            return 0.0;
        }

        mean_return / return_variance
    }

    /// Return the raw sortino ratio that has been derived from the sampled returns of the users balances.
    /// This sortino ratio is not annualized and does not include a risk free rate.
    pub fn sortino(&self) -> Option<f64> {
        let std_dev = self.user_balances_pos_ln_return_stats.last()?;
        let mean_pos_return = self.user_balances_pos_ln_return_stats.mean();

        // No risk free rate subtracted.
        Some(mean_pos_return / std_dev)
    }
}

impl<M> AccountTracker<M> for FullAccountTracker<M>
where
    M: Currency + MarginCurrency + Send,
{
    fn update(&mut self, market_state: &crate::prelude::MarketState) {
        if self.ts_first == 0.into() {
            self.ts_first = market_state.current_timestamp_ns();
        }
        self.ts_last = market_state.current_timestamp_ns();

        if self.price_first == quote!(0) {
            self.price_first = market_state.mid_price();
        }
        self.price_last = market_state.mid_price();

        self.drawdown_market
            .update(decimal_to_f64(*market_state.mid_price().as_ref()));
    }

    fn sample_user_balances(&mut self, user_balances: &UserBalances<M>) {
        let balance_sum = balance_sum(user_balances);
        self.last_balance_sum = balance_sum;

        let balance_sum = decimal_to_f64(*balance_sum.as_ref());
        self.user_balances_ln_return.update(balance_sum);
        self.drawdown_user_balances.update(balance_sum);

        if let Some(ln_ret) = self.user_balances_ln_return.last() {
            self.user_balances_ln_return_stats.update(ln_ret);
            if ln_ret > 0.0 {
                self.user_balances_pos_ln_return_stats.update(ln_ret);
            }
        }
    }

    fn log_fee(&mut self, fee_in_margin: M) {
        self.cumulative_fees += fee_in_margin
    }

    fn log_limit_order_submission(&mut self) {
        self.num_submitted_limit_orders += 1;
    }

    fn log_limit_order_cancellation(&mut self) {
        self.num_cancelled_limit_orders += 1;
    }

    fn log_limit_order_fill(&mut self) {
        self.num_fully_filled_limit_orders += 1;
    }

    fn log_market_order_fill(&mut self) {
        self.num_filled_market_orders += 1;
    }

    fn log_trade(&mut self, side: Side, price: QuoteCurrency, quantity: M::PairedCurrency) {
        assert!(quantity > M::PairedCurrency::new_zero());

        let value = quantity.convert(price);
        match side {
            Side::Buy => self.buy_volume += value,
            Side::Sell => self.sell_volume += value,
        }
    }

    fn log_market_order_submission(&mut self) {
        self.num_submitted_market_orders += 1;
    }
}

impl<M> Display for FullAccountTracker<M>
where
    M: Currency + MarginCurrency + Send,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "
rpnl: {},
sharpe: {:?},
sortino: {:?},
kelly_leverage: {},
buy_volume: {},
sell_volume: {},
turnover: {},
buy_and_hold_returns: {},
cumulative_fees: {},
num_trading_days: {},
limit_order_fill_ratio: {},
limit_order_cancellation_ratio: {},
            ",
            self.rpnl(),
            self.sharpe(),
            self.sortino(),
            self.kelly_leverage(),
            self.buy_volume,
            self.sell_volume,
            self.turnover(),
            self.buy_and_hold_return(),
            self.cumulative_fees(),
            self.num_trading_days(),
            self.limit_order_fill_ratio(),
            self.limit_order_cancellation_ratio(),
        )
    }
}

#[cfg(test)]
mod tests {
    use market_state::MarketState;

    use super::*;
    use crate::market_state;

    #[test]
    fn full_track_cumulative_fees() {
        let mut at = FullAccountTracker::new(quote!(100.0));
        at.log_fee(quote!(0.1));
        at.log_fee(quote!(0.2));
        assert_eq!(at.cumulative_fees(), quote!(0.3));
    }

    #[test]
    fn full_track_update() {
        let mut at = FullAccountTracker::new(quote!(1000));
        let market_state =
            MarketState::from_components(quote!(100), quote!(101), 1_000_000.into(), 0);
        at.update(&market_state);
        assert_eq!(at.num_submitted_limit_orders(), 0);
        assert_eq!(at.num_cancelled_limit_orders(), 0);
        assert_eq!(at.num_fully_filled_limit_orders(), 0);
        assert_eq!(at.num_submitted_market_orders(), 0);
        assert_eq!(at.num_filled_market_orders(), 0);

        assert_eq!(at.ts_first, 1_000_000.into());
        assert_eq!(at.ts_last, 1_000_000.into());
        assert_eq!(at.price_first, quote!(100.5));
        assert_eq!(at.price_last, quote!(100.5));
        assert_eq!(at.drawdown_market(), 0.0);
    }
}
