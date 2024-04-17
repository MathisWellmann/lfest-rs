use std::{fmt::Display, iter::FromIterator};

use fpdec::{Dec, Decimal};

use super::d_ratio;
use crate::{
    account_tracker::AccountTracker,
    cornish_fisher::cornish_fisher_value_at_risk,
    prelude::{Account, MarketState},
    quote,
    types::{Currency, LnReturns, MarginCurrency, QuoteCurrency, Side},
    utils::{decimal_pow, decimal_sqrt, decimal_sum, decimal_to_f64, min, variance},
};

const DAILY_NS: u64 = 86_400_000_000_000;
const HOURLY_NS: u64 = 3_600_000_000_000;

/// Defines the possible sources of returns to use
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ReturnsSource {
    /// Daily sampled returns
    Daily,
    /// Hourly sampled returns
    Hourly,
}

/// Keep track of many possible Account performance statistics
/// This can be quite memory intensive, easily reaching beyond 10GB
/// if using tick-by-tick data due to the storage of many returns
#[derive(Debug, Clone)]
pub struct FullAccountTracker<M> {
    wallet_balance_last: M,  // last wallet balance recording
    wallet_balance_start: M, // wallet balance at start
    wallet_balance_high: M,  // maximum wallet balance observed
    high_water_mark_ts: i64, // Timestamp of the maximum wallet balance
    total_rpnl: M,
    upnl: M,
    num_trades: i64,
    num_buys: i64,
    num_wins: usize,
    num_losses: usize,
    num_submitted_limit_orders: usize,
    num_cancelled_limit_orders: usize,
    num_limit_order_fills: usize,
    num_market_order_fills: usize,
    num_trading_opportunities: usize,
    total_turnover: M,
    max_drawdown_wallet_balance: Decimal,
    max_drawdown_total: Decimal,
    max_drawdown_duration_hours: i64,
    // historical daily absolute returns
    hist_returns_daily_acc: Vec<M>,
    hist_returns_daily_bnh: Vec<M>,
    // historical hourly absolute returns
    hist_returns_hourly_acc: Vec<M>,
    hist_returns_hourly_bnh: Vec<M>,
    // historical daily logarithmic returns
    hist_ln_returns_daily_acc: Vec<f64>,
    hist_ln_returns_daily_bnh: Vec<f64>,
    // historical hourly logarithmic returns
    hist_ln_returns_hourly_acc: Vec<f64>,
    hist_ln_returns_hourly_bnh: Vec<f64>,
    // timestamps for when to trigger the next pnl snapshots
    next_daily_trigger_ts: u64,
    next_hourly_trigger_ts: u64,
    last_daily_pnl: M,
    last_hourly_pnl: M,
    last_tick_pnl: M,
    cumulative_fees: M,
    total_profit: M,
    total_loss: M,
    price_first: QuoteCurrency,
    price_last: QuoteCurrency,
    price_a_day_ago: QuoteCurrency,
    price_an_hour_ago: QuoteCurrency,
    price_a_tick_ago: QuoteCurrency,
    ts_first: u64,
    ts_last: u64,
}

/// TODO: create its own `risk` crate out of these implementations for better
/// reusability and testability
impl<M> FullAccountTracker<M>
where
    M: Currency + MarginCurrency + Send,
{
    #[must_use]
    #[inline]
    /// Create a new AccTracker struct
    pub fn new(starting_wb: M) -> Self {
        FullAccountTracker {
            wallet_balance_last: starting_wb,
            wallet_balance_start: starting_wb,
            wallet_balance_high: starting_wb,
            high_water_mark_ts: 0,
            total_rpnl: M::new_zero(),
            upnl: M::new_zero(),
            num_trades: 0,
            num_buys: 0,
            num_wins: 0,
            num_losses: 0,
            num_submitted_limit_orders: 0,
            num_cancelled_limit_orders: 0,
            num_limit_order_fills: 0,
            num_market_order_fills: 0,
            num_trading_opportunities: 0,
            total_turnover: M::new_zero(),
            max_drawdown_wallet_balance: Decimal::from(0),
            max_drawdown_total: Decimal::from(0),
            max_drawdown_duration_hours: 0,
            hist_returns_daily_acc: vec![],
            hist_returns_daily_bnh: vec![],
            hist_returns_hourly_acc: vec![],
            hist_returns_hourly_bnh: vec![],
            hist_ln_returns_daily_acc: vec![],
            hist_ln_returns_daily_bnh: vec![],
            hist_ln_returns_hourly_acc: vec![],
            hist_ln_returns_hourly_bnh: vec![],
            next_daily_trigger_ts: 0,
            next_hourly_trigger_ts: 0,
            last_daily_pnl: M::new_zero(),
            last_hourly_pnl: M::new_zero(),
            last_tick_pnl: M::new_zero(),
            cumulative_fees: M::new_zero(),
            total_profit: M::new_zero(),
            total_loss: M::new_zero(),
            price_first: quote!(0.0),
            price_last: quote!(0.0),
            price_a_day_ago: quote!(0.0),
            price_an_hour_ago: quote!(0.0),
            price_a_tick_ago: quote!(0.0),
            ts_first: 0,
            ts_last: 0,
        }
    }

    /// Vector of absolute returns the account has generated, including
    /// unrealized pnl.
    ///
    /// # Parameters:
    /// `source`: the sampling interval of pnl snapshots
    ///
    /// TODO: rename for greater clarity
    #[inline(always)]
    pub fn absolute_returns(&self, source: &ReturnsSource) -> &Vec<M> {
        match source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
        }
    }

    /// Vector of natural logarithmic returns the account has generated,
    /// including unrealized pnl
    ///
    /// # Parameters:
    /// `source`: the sampling interval of pnl snapshots
    #[inline(always)]
    pub fn ln_returns(&self, source: &ReturnsSource) -> &Vec<f64> {
        match source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
        }
    }

    /// Ratio of cumulative trade profit over cumulative trade loss
    #[inline(always)]
    pub fn profit_loss_ratio(&self) -> Decimal {
        if self.total_loss == M::new_zero() {
            return Decimal::MAX;
        }
        (self.total_profit / self.total_loss).inner()
    }

    /// Cumulative fees paid to the exchange
    #[inline(always)]
    pub fn cumulative_fees(&self) -> M {
        self.cumulative_fees
    }

    /// Would be return of buy and hold strategy
    #[inline(always)]
    pub fn buy_and_hold_return(&self) -> M {
        let qty = self.wallet_balance_start.convert(self.price_first);
        M::pnl(self.price_first, self.price_last, qty)
    }

    /// Would be return of sell and hold strategy
    #[inline(always)]
    pub fn sell_and_hold_return(&self) -> M {
        self.buy_and_hold_return().into_negative()
    }

    /// Return the annualized sharpe ratio using a specific sampling frequency.
    ///
    /// # Parameters:
    /// `returns_source`: the sampling interval of pnl snapshots
    /// `risk_free_is_buy_and_hold`: if true, it will use the market returns as
    /// the risk-free comparison     else risk-free rate is zero
    pub fn sharpe(
        &self,
        returns_source: ReturnsSource,
        risk_free_is_buy_and_hold: bool,
    ) -> Decimal {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
        };
        let annualization_mult = match returns_source {
            ReturnsSource::Daily => Dec!(19.10497),  // sqrt(365)
            ReturnsSource::Hourly => Dec!(93.59487), // sqrt(365 * 24)
        };
        let n: Decimal = (rets_acc.len() as u64).into();
        let mean_ret_acc: Decimal = decimal_sum(rets_acc.iter().map(|v| v.inner())) / n;

        if risk_free_is_buy_and_hold {
            // Compute the mean buy and hold returns
            let rets_bnh = match returns_source {
                ReturnsSource::Daily => &self.hist_returns_daily_bnh,
                ReturnsSource::Hourly => &self.hist_returns_hourly_bnh,
            };
            let mean_bnh_ret = decimal_sum(rets_bnh.iter().map(|v| v.inner())) / n;

            // compute the difference of returns of account and market
            let diff_returns: Vec<Decimal> =
                rets_acc.iter().map(|v| v.inner() - mean_bnh_ret).collect();
            let var = variance(&diff_returns);
            if var == Decimal::ZERO {
                return Decimal::ZERO;
            }
            let std_dev = decimal_sqrt(var);

            annualization_mult * mean_ret_acc / std_dev
        } else {
            let var = variance(&rets_acc.iter().map(|v| v.inner()).collect::<Vec<_>>());
            if var == Decimal::ZERO {
                return Decimal::ZERO;
            }
            let std_dev = decimal_sqrt(var);

            annualization_mult * mean_ret_acc / std_dev
        }
    }

    /// Return the annualized Sortino ratio based on a specific sampling
    /// frequency.
    ///
    /// # Parameters:
    /// `returns_source`: the sampling interval of pnl snapshots
    /// `risk_free_is_buy_and_hold`: if true, it will use the market returns as
    /// the risk-free comparison     else risk-free rate is zero
    pub fn sortino(
        &self,
        returns_source: ReturnsSource,
        risk_free_is_buy_and_hold: bool,
    ) -> Decimal {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
        };
        if rets_acc.is_empty() {
            return Decimal::ZERO;
        }
        let annualization_mult = match returns_source {
            ReturnsSource::Daily => Dec!(19.10497),  // sqrt(365)
            ReturnsSource::Hourly => Dec!(93.59487), // sqrt(365 * 24)
        };

        let target_return: Decimal = if risk_free_is_buy_and_hold {
            let rets_bnh = match returns_source {
                ReturnsSource::Daily => &self.hist_returns_daily_bnh,
                ReturnsSource::Hourly => &self.hist_returns_hourly_bnh,
            };
            debug_assert!(
                !rets_bnh.is_empty(),
                "The buy and hold returns should not be empty at this point"
            );
            let n: Decimal = (rets_bnh.len() as u64).into();
            decimal_sum(rets_bnh.iter().map(|v| v.inner())) / n
        } else {
            Decimal::ZERO
        };

        let n: Decimal = (rets_acc.len() as u64).into();
        let mean_acc_ret = decimal_sum(rets_acc.iter().map(|v| v.inner())) / n;

        let underperformance = Vec::<Decimal>::from_iter(
            rets_acc
                .iter()
                .map(|v| decimal_pow(min(Decimal::ZERO, v.inner() - target_return), 2)),
        );

        let avg_underperformance = decimal_sum(underperformance.iter().cloned()) / n;

        let target_downside_deviation = decimal_sqrt(avg_underperformance);

        ((mean_acc_ret - target_return) * annualization_mult) / target_downside_deviation
    }

    /// Return the theoretical kelly leverage that would maximize the compounded growth rate,
    /// assuming the returns are normally distributed.
    /// WHICH THEY ARE NOT!
    /// I'd consider using the kelly leverage outright as "leveraging to the tits".
    /// Essentially: kelly f* = mean return / variance
    pub fn kelly_leverage(&self, returns_source: ReturnsSource) -> Decimal {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
        };
        let n: Decimal = (rets_acc.len() as u64).into();
        let mean_return = decimal_sum(rets_acc.iter().map(|v| v.inner())) / n;
        let rets_dec = Vec::<Decimal>::from_iter(rets_acc.iter().map(|v| v.inner()));
        let variance = variance(&rets_dec);
        if variance == Decimal::ZERO {
            return Decimal::ZERO;
        }

        mean_return / variance
    }

    /// Calculate the value at risk using the percentile method on daily returns
    /// multiplied by starting wallet balance The time horizon N is assumed
    /// to be 1 The literature says if you want a larger N, just multiply by
    /// N.sqrt(), which assumes standard normal distribution # Arguments
    /// returns_source: the sampling interval of pnl snapshots
    /// percentile: value between [0.0, 1.0], smaller value will return more
    /// worst case results
    /// TODO: annualized depending on the `ReturnsSource`
    #[inline]
    pub fn historical_value_at_risk(&self, returns_source: ReturnsSource, percentile: f64) -> f64 {
        let mut rets = match returns_source {
            ReturnsSource::Daily => self.hist_ln_returns_daily_acc.clone(),
            ReturnsSource::Hourly => self.hist_ln_returns_hourly_acc.clone(),
        };
        rets.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = (rets.len() as f64 * percentile) as usize;
        match rets.get(idx) {
            Some(r) => {
                decimal_to_f64(self.wallet_balance_start.inner())
                    - (decimal_to_f64(self.wallet_balance_start.inner()) * r.exp())
            }
            None => 0.0,
        }
    }

    /// Calculate the historical value at risk from n consequtive hourly return
    /// values, This should have better statistical properties compared to
    /// using daily returns due to having more samples. Set n to 24 for
    /// daily value at risk, but with 24x more samples from which to take the
    /// percentile, giving a more accurate VaR
    ///
    /// # Parameters:
    /// n: number of hourly returns to use
    /// percentile: value between [0.0, 1.0], smaller value will return more
    /// worst case results
    pub fn historical_value_at_risk_from_n_hourly_returns(&self, n: usize, percentile: f64) -> f64 {
        let rets = &self.hist_ln_returns_hourly_acc;
        if rets.len() < n {
            debug!("not enough hourly returns to compute VaR for n={}", n);
            return 0.0;
        }
        let mut ret_streaks = Vec::with_capacity(rets.len() - n);
        for i in n..rets.len() {
            let mut r = 1.0;
            for ret in rets.iter().take(i).skip(i - n) {
                r *= ret.exp();
            }
            ret_streaks.push(r);
        }

        ret_streaks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = (ret_streaks.len() as f64 * percentile) as usize;
        match ret_streaks.get(idx) {
            Some(r) => {
                decimal_to_f64(self.wallet_balance_start.inner())
                    - (decimal_to_f64(self.wallet_balance_start.inner()) * r)
            }
            None => 0.0,
        }
    }

    /// Calculate the cornish fisher value at risk of the account.
    ///
    /// # Arguments:
    /// - `returns_source`: the sampling interval of pnl snapshots
    /// - `percentile`: in range [0.0, 1.0], usually something like 0.01 or 0.05
    pub fn cornish_fisher_value_at_risk(
        &self,
        returns_source: ReturnsSource,
        percentile: f64,
    ) -> crate::Result<M> {
        let rets = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
        };
        Ok(
            cornish_fisher_value_at_risk(&LnReturns(rets), self.wallet_balance_start, percentile)?
                .asset_value_at_risk,
        )
    }

    /// Return the number of trading days
    #[inline(always)]
    pub fn num_trading_days(&self) -> u64 {
        (self.ts_last - self.ts_first) / DAILY_NS
    }

    /// Also called discriminant-ratio, which focuses on the added value of the
    /// algorithm It uses the Cornish-Fish Value at Risk (CF-VaR)
    /// It better captures the risk of the asset as it is not limited by the
    /// assumption of a gaussian distribution It it time-insensitive
    /// from: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3927058>
    ///
    /// # Parameters:
    /// `returns_source`: The sampling interval of pnl snapshots
    pub fn d_ratio(&self, returns_source: ReturnsSource) -> crate::Result<f64> {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
        };
        let rets_bnh = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_bnh,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_bnh,
        };
        d_ratio(
            LnReturns(rets_acc),
            LnReturns(rets_bnh),
            self.wallet_balance_start,
            self.num_trading_days(),
        )
    }

    /// Annualized return on investment as a factor, e.g.: 100% -> 2x
    pub fn annualized_roi(&self) -> Decimal {
        let num_trading_days = if self.num_trading_days() == 0 {
            1
        } else {
            self.num_trading_days() as u32
        };
        let power: u32 = 365 / num_trading_days;
        decimal_pow(
            Dec!(1) + self.total_rpnl.inner() / self.wallet_balance_start.inner(),
            power,
        )
    }

    /// Maximum drawdown of the wallet balance
    #[inline(always)]
    pub fn max_drawdown_wallet_balance(&self) -> Decimal {
        self.max_drawdown_wallet_balance
    }

    /// Maximum drawdown of the wallet balance including unrealized profit and
    /// loss
    #[inline(always)]
    pub fn max_drawdown_total(&self) -> Decimal {
        self.max_drawdown_total
    }

    /// The maximum duration the account balance was less than the high-water mark.
    /// This does not include unrealized profit and loss.
    /// The unit is hours.
    #[inline(always)]
    pub fn max_drawdown_duration_in_hours(&self) -> i64 {
        self.max_drawdown_duration_hours
    }

    /// Return the number of trades the account made
    #[inline(always)]
    pub fn num_trades(&self) -> i64 {
        self.num_trades
    }

    /// Return the number of submitted limit orders.
    #[inline(always)]
    pub fn num_submitted_limit_orders(&self) -> usize {
        self.num_submitted_limit_orders
    }

    /// Return the ratio of executed trades vs total trading opportunities
    /// Higher values means a more active trading agent
    #[inline(always)]
    pub fn trade_percentage(&self) -> f64 {
        self.num_trades as f64 / self.num_trading_opportunities as f64
    }

    /// Return the ratio of buy trades vs total number of trades
    #[inline(always)]
    pub fn buy_ratio(&self) -> f64 {
        self.num_buys as f64 / self.num_trades as f64
    }

    /// Return the cumulative turnover denoted in margin currency
    #[inline(always)]
    pub fn turnover(&self) -> M {
        self.total_turnover
    }

    /// Return the total realized profit and loss of the account
    #[inline(always)]
    pub fn total_rpnl(&self) -> M {
        self.total_rpnl
    }

    /// Return the current unrealized profit and loss
    #[inline(always)]
    pub fn upnl(&self) -> M {
        self.upnl
    }

    /// Return the ratio of winning trades vs all trades
    #[inline]
    pub fn win_ratio(&self) -> f64 {
        if self.num_wins + self.num_losses > 0 {
            self.num_wins as f64 / (self.num_wins + self.num_losses) as f64
        } else {
            0.0
        }
    }

    /// Return the ratio of filled limit orders vs number of submitted limit
    /// orders
    #[inline(always)]
    pub fn limit_order_fill_ratio(&self) -> f64 {
        self.num_limit_order_fills as f64 / self.num_submitted_limit_orders as f64
    }

    /// Return the ratio of limit order cancellations vs number of submitted
    /// limit orders
    #[inline(always)]
    pub fn limit_order_cancellation_ratio(&self) -> f64 {
        self.num_cancelled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

    /// The ratio of market order fills relative to total trades.
    #[inline(always)]
    pub fn market_order_trade_ratio(&self) -> f64 {
        self.num_market_order_fills as f64 / self.num_trades as f64
    }
}

impl<M> AccountTracker<M> for FullAccountTracker<M>
where
    M: Currency + MarginCurrency + Send,
{
    fn update(&mut self, timestamp_ns: u64, market_state: &MarketState, account: &Account<M>) {
        let price = market_state.mid_price();
        if price == quote!(0) {
            trace!("Price is 0, not updating the `FullAccountTracker`");
            return;
        }
        self.price_last = price;
        if self.price_a_day_ago.is_zero() {
            self.price_a_day_ago = price;
        }
        if self.price_an_hour_ago.is_zero() {
            self.price_an_hour_ago = price;
        }
        if self.price_a_tick_ago.is_zero() {
            self.price_a_tick_ago = price;
        }
        if self.price_first.is_zero() {
            self.price_first = price;
        }
        self.num_trading_opportunities += 1;
        if self.ts_first == 0 {
            self.ts_first = timestamp_ns;
        }
        self.ts_last = timestamp_ns;
        let upnl = account
            .position()
            .unrealized_pnl(market_state.bid(), market_state.ask());
        if timestamp_ns > self.next_daily_trigger_ts {
            self.next_daily_trigger_ts = timestamp_ns + DAILY_NS;

            // calculate daily return of account
            let pnl = (self.total_rpnl + upnl) - self.last_daily_pnl;
            self.hist_returns_daily_acc.push(pnl);

            // calculate daily log return of account
            let ln_ret: f64 = decimal_to_f64(
                ((self.wallet_balance_last + upnl)
                    / (self.wallet_balance_start + self.last_daily_pnl))
                    .inner(),
            )
            .ln();
            self.hist_ln_returns_daily_acc.push(ln_ret);

            // calculate daily return of buy_and_hold
            let bnh_qty = self.wallet_balance_start.convert(self.price_first);
            let pnl_bnh = M::pnl(self.price_a_day_ago, price, bnh_qty);
            self.hist_returns_daily_bnh.push(pnl_bnh);

            // calculate daily log return of market
            let ln_ret = decimal_to_f64((price / self.price_a_day_ago).inner()).ln();
            self.hist_ln_returns_daily_bnh.push(ln_ret);

            self.last_daily_pnl = self.total_rpnl + upnl;
            self.price_a_day_ago = price;
        }
        if timestamp_ns > self.next_hourly_trigger_ts {
            self.next_hourly_trigger_ts = timestamp_ns + HOURLY_NS;

            // calculate hourly return of account
            let pnl = (self.total_rpnl + upnl) - self.last_hourly_pnl;
            self.hist_returns_hourly_acc.push(pnl);

            // calculate hourly logarithmic return of account
            let ln_ret: f64 = decimal_to_f64(
                ((self.wallet_balance_last + upnl)
                    / (self.wallet_balance_start + self.last_hourly_pnl))
                    .inner(),
            )
            .ln();
            self.hist_ln_returns_hourly_acc.push(ln_ret);

            // calculate hourly return of buy_and_hold
            let bnh_qty = self.wallet_balance_start.convert(self.price_first);
            let pnl_bnh = M::pnl(self.price_an_hour_ago, price, bnh_qty);
            self.hist_returns_hourly_bnh.push(pnl_bnh);

            // calculate hourly logarithmic return of buy_and_hold
            let ln_ret = decimal_to_f64((price / self.price_an_hour_ago).inner()).ln();
            self.hist_ln_returns_hourly_bnh.push(ln_ret);

            self.last_hourly_pnl = self.total_rpnl + upnl;
            self.price_an_hour_ago = price;
        }

        self.last_tick_pnl = self.total_rpnl + upnl;
        self.price_a_tick_ago = price;

        // update max_drawdown_total
        let curr_dd = (self.wallet_balance_high - (self.wallet_balance_last + upnl))
            / self.wallet_balance_high;
        let curr_dd = curr_dd.inner();
        if curr_dd > self.max_drawdown_total {
            self.max_drawdown_total = curr_dd;
        }

        // update max drawdown duration
        self.max_drawdown_duration_hours =
            (timestamp_ns as i64 - self.high_water_mark_ts) / HOURLY_NS as i64;
    }

    fn log_rpnl(&mut self, net_rpnl: M, ts_ns: i64) {
        self.total_rpnl += net_rpnl;
        self.wallet_balance_last += net_rpnl;
        if net_rpnl < M::new_zero() {
            self.total_loss += net_rpnl.abs();
            self.num_losses += 1;
        } else {
            self.num_wins += 1;
            self.total_profit += net_rpnl;
        }
        if self.wallet_balance_last > self.wallet_balance_high {
            self.wallet_balance_high = self.wallet_balance_last;
            self.high_water_mark_ts = ts_ns;
        }
        let dd = (self.wallet_balance_high - self.wallet_balance_last) / self.wallet_balance_high;
        let dd = dd.inner();
        if dd > self.max_drawdown_wallet_balance {
            self.max_drawdown_wallet_balance = dd;
        }
    }

    #[inline(always)]
    fn log_fee(&mut self, fee_in_margin: M) {
        self.cumulative_fees += fee_in_margin
    }

    #[inline(always)]
    fn log_limit_order_submission(&mut self) {
        self.num_submitted_limit_orders += 1;
    }

    #[inline(always)]
    fn log_limit_order_cancellation(&mut self) {
        self.num_cancelled_limit_orders += 1;
    }

    #[inline(always)]
    fn log_limit_order_fill(&mut self) {
        self.num_limit_order_fills += 1;
    }

    #[inline(always)]
    fn log_market_order_fill(&mut self) {
        self.num_market_order_fills += 1;
    }

    fn log_trade(&mut self, side: Side, price: QuoteCurrency, quantity: M::PairedCurrency) {
        self.total_turnover += quantity.abs().convert(price);
        self.num_trades += 1;
        if let Side::Buy = side {
            self.num_buys += 1
        }
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
annualized_roi: {},
sharpe_daily_returns: {},
sharpe_hourly_returns: {},
sortino_daily_returns: {},
sortino_hourly_returns: {},
drawdown_wallet_balance: {},
drawdown_total: {},
historical_value_at_risk_daily: {},
historical_value_at_risk_hourly: {},
cornish_fisher_value_at_risk_daily: {:?},
d_ratio_daily: {:?},
d_ratio_hourly: {:?},
num_trades: {},
buy_ratio: {},
turnover: {},
win_ratio: {},
profit_loss_ratio: {},
buy_and_hold_returns: {},
trade_percentage: {},
cumulative_fees: {},
num_trading_days: {},
            ",
            self.total_rpnl(),
            self.annualized_roi(),
            self.sharpe(ReturnsSource::Daily, true),
            self.sharpe(ReturnsSource::Hourly, true),
            self.sortino(ReturnsSource::Daily, true),
            self.sortino(ReturnsSource::Hourly, true),
            self.max_drawdown_wallet_balance(),
            self.max_drawdown_total(),
            self.historical_value_at_risk(ReturnsSource::Daily, 0.01),
            self.historical_value_at_risk_from_n_hourly_returns(24, 0.01),
            self.cornish_fisher_value_at_risk(ReturnsSource::Daily, 0.01),
            self.d_ratio(ReturnsSource::Daily),
            self.d_ratio(ReturnsSource::Hourly),
            self.num_trades(),
            self.buy_ratio(),
            self.turnover(),
            self.win_ratio(),
            self.profit_loss_ratio(),
            self.buy_and_hold_return(),
            self.trade_percentage(),
            self.cumulative_fees(),
            self.num_trading_days(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use fpdec::Round;

    use super::*;
    use crate::{
        prelude::PriceFilter,
        test_helpers::LN_RETS_H,
        utils::{f64_to_decimal, tests::round},
    };

    // Example pulled from the following article about the Sortino ratio:
    // http://www.redrockcapital.com/Sortino__A__Sharper__Ratio_Red_Rock_Capital.pdf
    const ACC_RETS_H: [f64; 8] = [0.17, 0.15, 0.23, -0.05, 0.12, 0.09, 0.13, -0.04];

    fn mock_market_state_from_mid_price(mid_price: QuoteCurrency) -> MarketState {
        MarketState::from_components(PriceFilter::default(), mid_price, mid_price, 0, 0)
    }

    #[test]
    fn acc_tracker_profit_loss_ratio() {
        let mut at = FullAccountTracker::new(quote!(100.0));
        at.total_profit = quote!(50.0);
        at.total_loss = quote!(25.0);
        assert_eq!(at.profit_loss_ratio(), Decimal::TWO);
    }

    #[test]
    fn acc_tracker_cumulative_fees() {
        let mut at = FullAccountTracker::new(quote!(100.0));
        at.log_fee(quote!(0.1));
        at.log_fee(quote!(0.2));
        assert_eq!(at.cumulative_fees(), quote!(0.3));
    }

    #[test]
    fn acc_tracker_buy_and_hold_return() {
        let mut at = FullAccountTracker::new(quote!(100.0));
        at.update(
            0,
            &mock_market_state_from_mid_price(quote!(100.0)),
            &Account::default(),
        );
        at.update(
            0,
            &mock_market_state_from_mid_price(quote!(200.0)),
            &Account::default(),
        );
        assert_eq!(at.buy_and_hold_return(), quote!(100.0));
    }

    #[test]
    fn acc_tracker_sell_and_hold_return() {
        let mut at = FullAccountTracker::new(quote!(100.0));
        at.update(
            0,
            &mock_market_state_from_mid_price(quote!(100.0)),
            &Account::default(),
        );
        at.update(
            0,
            &mock_market_state_from_mid_price(quote!(50.0)),
            &Account::default(),
        );
        assert_eq!(at.sell_and_hold_return(), quote!(50.0));
    }

    #[test]
    fn acc_tracker_log_rpnl() {
        let rpnls: Vec<Decimal> = [1, -1, 1, 2, -1]
            .iter()
            .map(|v| Decimal::from(*v))
            .collect();
        let mut acc_tracker = FullAccountTracker::new(quote!(100.0));
        for r in rpnls {
            acc_tracker.log_rpnl(QuoteCurrency::new(r), 0);
        }

        assert_eq!(
            acc_tracker.max_drawdown_wallet_balance().round(2),
            Decimal::try_from(0.01).unwrap()
        );
        assert_eq!(acc_tracker.total_rpnl(), quote!(2.0));
    }

    #[test]
    fn acc_tracker_buy_and_hold() {
        let mut acc_tracker = FullAccountTracker::new(quote!(100.0));
        acc_tracker.update(
            0,
            &mock_market_state_from_mid_price(quote!(100.0)),
            &Account::default(),
        );
        acc_tracker.update(
            0,
            &mock_market_state_from_mid_price(quote!(200.0)),
            &Account::default(),
        );
        assert_eq!(acc_tracker.buy_and_hold_return(), quote!(100.0));
    }

    #[test]
    fn acc_tracker_sell_and_hold() {
        let mut acc_tracker = FullAccountTracker::new(quote!(100.0));
        acc_tracker.update(
            0,
            &mock_market_state_from_mid_price(quote!(100.0)),
            &Account::default(),
        );
        acc_tracker.update(
            0,
            &mock_market_state_from_mid_price(quote!(200.0)),
            &Account::default(),
        );
        assert_eq!(acc_tracker.sell_and_hold_return(), quote!(-100.0));
    }

    #[test]
    fn acc_tracker_historical_value_at_risk() {
        if let Err(_e) = pretty_env_logger::try_init() {}

        let mut acc_tracker = FullAccountTracker::new(quote!(100.0));
        acc_tracker.hist_ln_returns_hourly_acc = LN_RETS_H.into();

        assert_eq!(
            round(
                acc_tracker.historical_value_at_risk(ReturnsSource::Hourly, 0.05),
                3
            ),
            1.173
        );
        assert_eq!(
            round(
                acc_tracker.historical_value_at_risk(ReturnsSource::Hourly, 0.01),
                3
            ),
            2.54
        );
    }

    #[test]
    fn acc_tracker_historical_value_at_risk_from_n_hourly_returns() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let mut at = FullAccountTracker::new(quote!(100.0));
        at.hist_ln_returns_hourly_acc = LN_RETS_H.into();

        assert_eq!(
            round(
                at.historical_value_at_risk_from_n_hourly_returns(24, 0.05),
                3
            ),
            3.835
        );
        assert_eq!(
            round(
                at.historical_value_at_risk_from_n_hourly_returns(24, 0.01),
                3
            ),
            6.061
        );
    }

    #[test]
    fn acc_tracker_cornish_fisher_value_at_risk() {
        if let Err(_e) = pretty_env_logger::try_init() {}

        let mut acc_tracker = FullAccountTracker::new(quote!(100.0));
        acc_tracker.hist_ln_returns_hourly_acc = LN_RETS_H.into();

        assert_eq!(
            round(
                decimal_to_f64(
                    acc_tracker
                        .cornish_fisher_value_at_risk(ReturnsSource::Hourly, 0.05)
                        .unwrap()
                        .inner()
                ),
                3
            ),
            98.646
        );
        assert_eq!(
            round(
                decimal_to_f64(
                    acc_tracker
                        .cornish_fisher_value_at_risk(ReturnsSource::Hourly, 0.01)
                        .unwrap()
                        .inner()
                ),
                3
            ),
            94.214
        );
    }

    #[test]
    fn acc_tracker_sortino() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let mut at = FullAccountTracker::new(quote!(100.0));

        at.hist_returns_hourly_acc = Vec::<QuoteCurrency>::from_iter(
            ACC_RETS_H
                .iter()
                .map(|v| QuoteCurrency::new(f64_to_decimal(*v, Dec!(0.001)))),
        );

        const EXPECTED_SORTINO_RATIO: Decimal = Dec!(413.434120785921266504);

        assert!(
            at.sortino(ReturnsSource::Hourly, false) - EXPECTED_SORTINO_RATIO
                < Dec!(0.0000000000000001),
        );
    }
}
