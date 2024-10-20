use std::fmt::Display;

use getset::CopyGetters;
use num_traits::Zero;
use sliding_features::{
    pure_functions::Echo,
    rolling::{Drawdown, LnReturn, WelfordRolling},
    View,
};

use crate::{
    account_tracker::AccountTracker,
    prelude::{MarketState, Mon, QuoteCurrency, Side, UserBalances},
    types::{CurrencyMarker, MarginCurrencyMarker, TimestampNs},
    utils::balance_sum,
};

const DAILY_NS: i64 = 86_400_000_000_000;

/// Keep track of Account performance statistics.
#[derive(Debug, CopyGetters)]
pub struct FullAccountTracker<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
{
    /// Wallet balance at the start.
    #[getset(get_copy = "pub")]
    wallet_balance_start: BaseOrQuote,

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
    buy_volume: BaseOrQuote,

    /// The total volume sold.
    #[getset(get_copy = "pub")]
    sell_volume: BaseOrQuote,

    price_first: QuoteCurrency<I, D>,
    price_last: QuoteCurrency<I, D>,
    ts_first: TimestampNs,
    ts_last: TimestampNs,

    /// Keep track of natural logarithmic returns of users funds.
    user_balances_ln_return: LnReturn<f32, Echo<f32>>,
    drawdown_user_balances: Drawdown<f32, Echo<f32>>, // Drawdown of realized user balances.
    drawdown_market: Drawdown<f32, Echo<f32>>,        // Drawdown of the market.
    user_balances_ln_return_stats: WelfordRolling<f32, Echo<f32>>, // Used for `sharpe` and `kelly_leverage`
    user_balances_neg_ln_return_stats: WelfordRolling<f32, Echo<f32>>, // Used for `sortino`

    /// last sum of all user balances.
    last_balance_sum: BaseOrQuote,

    /// Keeps track of ln return distribution of user balances and can compute the quantiles needed for certain risk metrics.
    #[cfg(feature = "quantiles")]
    quantogram_user_balances_ln_returns: quantogram::Quantogram,

    /// Keeps track of the markets logarithmic return at the sampling interval.
    #[cfg(feature = "quantiles")]
    sampled_market_ln_return: LnReturn<f32, Echo<f32>>,

    /// Keeps track of ln return distribution of the market and can compute the quantiles needed for certain risk metrics.
    #[cfg(feature = "quantiles")]
    quantogram_market_ln_returns: quantogram::Quantogram,
}

/// TODO: create its own `risk` crate out of these implementations for better
/// reusability and testability
impl<I, const D: u8, BaseOrQuote> FullAccountTracker<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
{
    /// Create a new instance of `Self`.
    #[must_use]
    pub fn new(starting_wb: BaseOrQuote) -> Self {
        assert!(
            starting_wb > BaseOrQuote::zero(),
            "The starting wallet balance must be greater than zero"
        );

        FullAccountTracker {
            wallet_balance_start: starting_wb,

            num_submitted_limit_orders: 0,
            num_cancelled_limit_orders: 0,
            num_fully_filled_limit_orders: 0,
            num_submitted_market_orders: 0,
            num_filled_market_orders: 0,

            buy_volume: BaseOrQuote::zero(),
            sell_volume: BaseOrQuote::zero(),

            price_first: QuoteCurrency::zero(),
            price_last: QuoteCurrency::zero(),
            ts_first: TimestampNs::from(0),
            ts_last: TimestampNs::from(0),

            user_balances_ln_return: LnReturn::default(),
            drawdown_user_balances: Drawdown::default(),
            drawdown_market: Drawdown::default(),
            user_balances_ln_return_stats: WelfordRolling::default(),
            user_balances_neg_ln_return_stats: WelfordRolling::default(),

            last_balance_sum: BaseOrQuote::zero(),

            #[cfg(feature = "quantiles")]
            quantogram_user_balances_ln_returns: quantogram::QuantogramBuilder::new()
                .with_error(0.001)
                .build(),

            #[cfg(feature = "quantiles")]
            sampled_market_ln_return: LnReturn::default(),

            #[cfg(feature = "quantiles")]
            quantogram_market_ln_returns: quantogram::QuantogramBuilder::new()
                .with_error(0.001)
                .build(),
        }
    }

    /// Would be the return of buy and hold strategy
    pub fn buy_and_hold_return(&self) -> BaseOrQuote {
        let qty =
            BaseOrQuote::PairedCurrency::convert_from(self.wallet_balance_start, self.price_first);
        BaseOrQuote::pnl(self.price_first, self.price_last, qty)
    }

    /// Would be the return of sell and hold strategy
    pub fn sell_and_hold_return(&self) -> BaseOrQuote {
        self.buy_and_hold_return().neg()
    }

    /// Return the number of trading days
    pub fn num_trading_days(&self) -> u32 {
        assert!(
            self.ts_last >= self.ts_first,
            "Last timestamp must be after first."
        );

        Into::<i64>::into((self.ts_last - self.ts_first) / TimestampNs::from(DAILY_NS)) as u32
    }

    /// Return the ratio of filled limit orders vs number of submitted limit
    /// orders
    pub fn limit_order_fill_ratio(&self) -> f32 {
        self.num_fully_filled_limit_orders as f32 / self.num_submitted_limit_orders as f32
    }

    /// Return the ratio of limit order cancellations vs number of submitted
    /// limit orders
    pub fn limit_order_cancellation_ratio(&self) -> f32 {
        self.num_cancelled_limit_orders as f32 / self.num_submitted_limit_orders as f32
    }

    /// The total volume traded.
    pub fn turnover(&self) -> BaseOrQuote {
        self.buy_volume + self.sell_volume
    }

    /// The drawdown of user balances.
    pub fn drawdown_user_balances(&self) -> f32 {
        self.drawdown_user_balances.last().unwrap_or(0.0)
    }

    /// The drawdown the market experienced.
    pub fn drawdown_market(&self) -> f32 {
        self.drawdown_market.last().unwrap_or(0.0)
    }

    /// The realized profit and loss of the users account.
    /// Unrealized pnl not included.
    pub fn rpnl(&self) -> BaseOrQuote {
        self.last_balance_sum - self.wallet_balance_start
    }

    /// The ratio of executed buy volume vs total.
    pub fn buy_volume_ratio(&self) -> Option<f32> {
        assert!(self.buy_volume >= BaseOrQuote::zero());
        assert!(self.sell_volume >= BaseOrQuote::zero());

        let total_volume = self.buy_volume + self.sell_volume;
        if total_volume.is_zero() {
            return None;
        }

        Some(Into::<f64>::into(self.buy_volume / total_volume) as f32)
    }

    /// Return the raw sharpe ratio that has been derived from the sampled returns of the users balances.
    /// This sharpe ratio is not annualized and does not include a risk free rate.
    pub fn sharpe(&self) -> Option<f32> {
        let std_dev = self.user_balances_ln_return_stats.last()?;
        if std_dev == 0.0 {
            return None;
        }
        let mean_return = self.user_balances_ln_return_stats.mean();

        // No risk free rate subtracted.
        Some(mean_return / std_dev)
    }

    /// Returns the theoretical kelly leverage that would maximize the compounded growth rate,
    /// assuming the returns are normally distributed. Which they almost never are. So be aware.
    pub fn kelly_leverage(&self) -> f32 {
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
    pub fn sortino(&self) -> Option<f32> {
        let neg_std_dev = self.user_balances_neg_ln_return_stats.last()?;
        if neg_std_dev == 0.0 {
            return None;
        }
        let mean_return = self.user_balances_ln_return_stats.mean();

        // No risk free rate subtracted.
        Some(mean_return / neg_std_dev)
    }

    /// The discriminant ratio (`d_ratio`) divides the return-to-VaR ratio of the user performance
    /// by the return-to-VaR ratio of the buy-and-hold strategy.
    /// If the `d_ratio` is greater than 1, the user outperformed the buy-and-hold strategy.
    /// from: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3927058>
    #[cfg(feature = "quantiles")]
    pub fn d_ratio(&self, quantile: f64) -> Option<f64> {
        let market_quantile = self.quantogram_market_ln_returns.quantile(quantile)?;
        let user_balances_quantile = self
            .quantogram_user_balances_ln_returns
            .quantile(quantile)?;

        let market_mean_return = self.quantogram_market_ln_returns.mean()?;
        let user_balance_mean_return = self.quantogram_user_balances_ln_returns.mean()?;

        let rtv_algo = user_balance_mean_return / user_balances_quantile.abs();
        let rtv_bnh = market_mean_return / market_quantile.abs();

        Some(1.0 + (rtv_algo - rtv_bnh) / (rtv_bnh).abs())
    }
}

impl<I, const D: u8, BaseOrQuote> AccountTracker<I, D, BaseOrQuote>
    for FullAccountTracker<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
{
    fn update(&mut self, market_state: &MarketState<I, D>) {
        if self.ts_first == 0.into() {
            self.ts_first = market_state.current_timestamp_ns();
        }
        self.ts_last = market_state.current_timestamp_ns();

        if self.price_first.is_zero() {
            self.price_first = market_state.mid_price();
        }
        self.price_last = market_state.mid_price();

        self.drawdown_market
            .update(Into::<f64>::into(market_state.mid_price()) as f32);
    }

    fn sample_user_balances(
        &mut self,
        user_balances: &UserBalances<BaseOrQuote>,
        #[allow(unused)] mid_price: QuoteCurrency<I, D>,
    ) {
        let balance_sum = balance_sum(user_balances);
        self.last_balance_sum = balance_sum;

        let balance_sum = Into::<f64>::into(balance_sum) as f32;
        self.drawdown_user_balances.update(balance_sum);

        self.user_balances_ln_return.update(balance_sum);
        if let Some(ln_ret) = self.user_balances_ln_return.last() {
            self.user_balances_ln_return_stats.update(ln_ret);
            if ln_ret < 0.0 {
                self.user_balances_neg_ln_return_stats.update(ln_ret);
            }
            #[cfg(feature = "quantiles")]
            self.quantogram_user_balances_ln_returns.add(ln_ret as f64);
        }

        #[cfg(feature = "quantiles")]
        {
            let mid_price = mid_price.as_ref().to_f64() as f32;
            self.sampled_market_ln_return.update(mid_price);
            if let Some(market_ln_ret) = self.sampled_market_ln_return.last() {
                self.quantogram_market_ln_returns.add(market_ln_ret as f64);
            }
        }
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

    fn log_trade(
        &mut self,
        side: Side,
        price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote::PairedCurrency,
    ) {
        assert!(quantity > BaseOrQuote::PairedCurrency::zero());

        let value = BaseOrQuote::convert_from(quantity, price);
        match side {
            Side::Buy => self.buy_volume += value,
            Side::Sell => self.sell_volume += value,
        }
    }

    fn log_market_order_submission(&mut self) {
        self.num_submitted_market_orders += 1;
    }
}

impl<I, const D: u8, BaseOrQuote> Display for FullAccountTracker<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrencyMarker<I, D>,
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
    use crate::{market_state, prelude::*};

    #[test]
    fn full_track_update() {
        let mut at = FullAccountTracker::new(QuoteCurrency::<i64, 4>::new(1000, 0));
        let market_state = MarketState::from_components(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            1_000_000.into(),
            0,
        );
        at.update(&market_state);
        assert_eq!(at.num_submitted_limit_orders(), 0);
        assert_eq!(at.num_cancelled_limit_orders(), 0);
        assert_eq!(at.num_fully_filled_limit_orders(), 0);
        assert_eq!(at.num_submitted_market_orders(), 0);
        assert_eq!(at.num_filled_market_orders(), 0);

        assert_eq!(at.ts_first, 1_000_000.into());
        assert_eq!(at.ts_last, 1_000_000.into());
        assert_eq!(at.price_first, QuoteCurrency::new(1005, 1));
        assert_eq!(at.price_last, QuoteCurrency::new(1005, 1));
        assert_eq!(at.drawdown_market(), 0.0);
    }

    #[test]
    fn full_track_sharpe() {
        let mut at = FullAccountTracker::new(QuoteCurrency::<i64, 4>::new(1000, 0));
        let balances = UserBalances {
            available_wallet_balance: QuoteCurrency::new(100, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero(),
        };
        at.sample_user_balances(&balances, QuoteCurrency::new(100, 0));

        let balances = UserBalances {
            available_wallet_balance: QuoteCurrency::new(101, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero(),
        };
        at.sample_user_balances(&balances, QuoteCurrency::new(100, 0));
        assert_eq!(at.user_balances_ln_return.last().unwrap(), 0.009950321);
        assert_eq!(at.drawdown_user_balances(), 0.0);
        assert_eq!(at.user_balances_ln_return_stats.last().unwrap(), 0.0);
        assert!(at.sharpe().is_none());
        assert!(at.sortino().is_none());
        assert_eq!(at.kelly_leverage(), 0.0);

        let balances = UserBalances {
            available_wallet_balance: QuoteCurrency::new(102, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero(),
        };
        at.sample_user_balances(&balances, QuoteCurrency::new(100, 0));
        assert_eq!(at.user_balances_ln_return.last().unwrap(), 0.009852353);
        assert_eq!(at.user_balances_ln_return_stats.mean(), 0.009901337);
        assert_eq!(at.user_balances_ln_return_stats.variance(), 2.3994853e-9);
        assert_eq!(
            at.user_balances_ln_return_stats.last().unwrap(),
            4.898454e-5
        );
        assert_eq!(at.drawdown_user_balances(), 0.0);
        assert_eq!(at.sharpe().unwrap(), 202.13188);
        assert!(at.sortino().is_none());
        assert_eq!(at.kelly_leverage(), 4126442.3);
    }
}
