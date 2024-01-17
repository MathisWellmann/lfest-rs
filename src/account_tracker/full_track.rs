use std::{fmt::Display, iter::FromIterator};

use fpdec::{Dec, Decimal};

use crate::{
    account_tracker::AccountTracker,
    cornish_fisher::cornish_fisher_value_at_risk,
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency, Side},
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

    /// Calculate the cornish fisher value at risk based on daily returns of the
    /// account # Arguments
    /// returns_source: the sampling interval of pnl snapshots
    /// percentile: in range [0.0, 1.0], usually something like 0.01 or 0.05
    #[inline]
    pub fn cornish_fisher_value_at_risk(
        &self,
        returns_source: ReturnsSource,
        percentile: f64,
    ) -> f64 {
        let rets = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
        };
        cornish_fisher_value_at_risk(
            rets,
            decimal_to_f64(self.wallet_balance_start.inner()),
            percentile,
        )
        .2
    }

    /// Calculate the corni fisher value at risk from n consequtive hourly
    /// return values This should have better statistical properties
    /// compared to using daily returns due to having more samples. Set n to
    /// 24 for daily value at risk, but with 24x more samples from which to take
    /// the percentile, giving a more accurate VaR
    /// # Parameters:
    /// n: number of hourly returns to use
    /// percentile: value between [0.0, 1.0], smaller value will return more
    /// worst case results
    pub fn cornish_fisher_value_at_risk_from_n_hourly_returns(
        &self,
        n: usize,
        percentile: f64,
    ) -> f64 {
        let rets = &self.hist_ln_returns_hourly_acc;
        if rets.len() < n {
            debug!("not enough hourly returns to compute CF-VaR for n={}", n);
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

        // TODO: make work with `Decimal` type
        let cf_var = cornish_fisher_value_at_risk(
            &ret_streaks,
            decimal_to_f64(self.wallet_balance_start.inner()),
            percentile,
        )
        .1;
        decimal_to_f64(self.wallet_balance_start.inner())
            - (decimal_to_f64(self.wallet_balance_start.inner()) * cf_var)
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
    pub fn d_ratio(&self, returns_source: ReturnsSource) -> f64 {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
        };
        let rets_bnh = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_bnh,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_bnh,
        };

        let cf_var_bnh = cornish_fisher_value_at_risk(
            rets_bnh,
            decimal_to_f64(self.wallet_balance_start.inner()),
            0.01,
        )
        .1;
        let cf_var_acc = cornish_fisher_value_at_risk(
            rets_acc,
            decimal_to_f64(self.wallet_balance_start.inner()),
            0.01,
        )
        .1;

        let num_trading_days = self.num_trading_days() as f64;

        // compute annualized returns
        let roi_acc = rets_acc
            .iter()
            .fold(1.0, |acc, x| acc * x.exp())
            .powf(365.0 / num_trading_days);
        let roi_bnh = rets_bnh
            .iter()
            .fold(1.0, |acc, x| acc * x.exp())
            .powf(365.0 / num_trading_days);

        let rtv_acc = roi_acc / cf_var_acc;
        let rtv_bnh = roi_bnh / cf_var_bnh;
        debug!(
            "roi_acc: {:.2}, roi_bnh: {:.2}, cf_var_bnh: {:.8}, cf_var_acc: {:.8}, rtv_acc: {}, rtv_bnh: {}",
            roi_acc, roi_bnh, cf_var_bnh, cf_var_acc, rtv_acc, rtv_bnh,
        );

        (1.0 + (roi_acc - roi_bnh) / roi_bnh.abs()) * (cf_var_bnh / cf_var_acc)
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
    fn update(&mut self, timestamp_ns: u64, price: QuoteCurrency, upnl: M) {
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
cornish_fisher_value_at_risk_daily: {},
cornish_fisher_value_at_risk_daily_from_hourly_returns: {},
d_ratio_daily: {},
d_ratio_hourly: {},
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
            self.cornish_fisher_value_at_risk_from_n_hourly_returns(24, 0.01),
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
    use crate::utils::{f64_to_decimal, tests::round};

    // Example pulled from the following article about the Sortino ratio:
    // http://www.redrockcapital.com/Sortino__A__Sharper__Ratio_Red_Rock_Capital.pdf
    const ACC_RETS_H: [f64; 8] = [0.17, 0.15, 0.23, -0.05, 0.12, 0.09, 0.13, -0.04];

    // Some example hourly ln returns of BCHEUR i pulled from somewhere from about
    // october 2021
    const LN_RETS_H: [f64; 400] = [
        0.00081502,
        0.00333945,
        0.01293622,
        -0.00477679,
        -0.01195175,
        0.00750783,
        0.00426066,
        0.01214974,
        0.00892472,
        0.00344957,
        0.00684050,
        -0.00492310,
        0.00322274,
        0.02181239,
        0.00592118,
        0.00122343,
        -0.00623743,
        -0.00273835,
        0.01127133,
        -0.07646319,
        0.07090849,
        -0.00494601,
        -0.00624408,
        0.00256976,
        0.00130659,
        0.00098106,
        -0.00635020,
        0.00191424,
        -0.00306103,
        0.00640057,
        -0.00550237,
        0.00469525,
        0.00207676,
        -0.00449422,
        0.00472523,
        -0.00459109,
        -0.00382578,
        0.00420916,
        -0.01085029,
        0.00277287,
        -0.00929482,
        0.00680648,
        -0.00772934,
        -0.00250064,
        -0.01213199,
        -0.00098276,
        -0.00441975,
        0.00118162,
        0.00318254,
        -0.00314559,
        -0.00210387,
        0.00452694,
        -0.00116603,
        -0.00240180,
        0.00188400,
        0.00442843,
        -0.00769548,
        0.00154913,
        0.00447643,
        0.00081605,
        -0.00081605,
        -0.00201872,
        0.00183335,
        0.00540848,
        -0.01165400,
        0.00293312,
        0.00133104,
        -0.00555275,
        0.00309541,
        -0.01556380,
        -0.00101692,
        -0.00094336,
        -0.00039885,
        0.00121517,
        0.00312631,
        -0.00452272,
        -0.00484508,
        0.00718562,
        0.00252812,
        -0.00085555,
        0.00582124,
        0.00917446,
        -0.00847876,
        0.00492033,
        -0.00139778,
        -0.00511463,
        0.00474712,
        -0.00256881,
        0.00185255,
        -0.00276838,
        -0.00118933,
        0.01393963,
        0.00211617,
        -0.00733174,
        0.00223456,
        0.00331485,
        -0.00812862,
        0.00127036,
        0.01245729,
        -0.01264150,
        0.00075547,
        -0.00219115,
        0.00163830,
        -0.00734218,
        0.00730533,
        -0.00090229,
        -0.00585425,
        0.00370310,
        -0.00388606,
        0.00350045,
        -0.00593072,
        0.00756601,
        0.02024774,
        0.01012805,
        0.00128986,
        -0.00030365,
        -0.01334484,
        -0.00177715,
        -0.00373107,
        0.00792646,
        0.00013139,
        -0.00342925,
        0.01376916,
        0.00051222,
        0.00475530,
        -0.01058291,
        -0.00384123,
        -0.00663085,
        0.00141987,
        -0.00084096,
        -0.00953725,
        -0.00181163,
        -0.00127357,
        0.00040589,
        -0.00053500,
        0.00271486,
        -0.00024039,
        0.00613869,
        -0.00222986,
        -0.00340949,
        -0.00190351,
        0.00934898,
        0.00117479,
        -0.00102569,
        0.00003728,
        0.00257564,
        0.00893534,
        -0.00150733,
        -0.00645575,
        -0.00572640,
        0.00951222,
        -0.02857972,
        0.00519596,
        0.00908435,
        -0.00122096,
        -0.00510812,
        0.00103059,
        -0.00003682,
        -0.00266620,
        0.00473049,
        0.00377094,
        0.03262131,
        -0.00294230,
        -0.00281953,
        -0.00362701,
        -0.00001896,
        0.00212520,
        0.00367280,
        -0.00188566,
        0.00647177,
        -0.00816393,
        0.00705369,
        0.00903244,
        -0.00235244,
        0.01674118,
        -0.00652002,
        0.02306826,
        0.00615165,
        0.00122285,
        -0.00276431,
        0.00962792,
        0.01871500,
        -0.00793240,
        0.00881768,
        0.00592885,
        0.02721942,
        0.00850996,
        -0.01381862,
        0.00936217,
        -0.00407480,
        0.00236606,
        -0.00513002,
        0.01970497,
        -0.01412668,
        0.01755395,
        -0.00895548,
        0.00511687,
        0.00296984,
        0.02988059,
        -0.02572539,
        -0.00835808,
        0.00918683,
        0.00781964,
        0.00013195,
        -0.00880214,
        -0.01109966,
        -0.00734618,
        0.00665653,
        -0.01180100,
        0.00818809,
        0.00311751,
        -0.00260218,
        0.00804343,
        -0.00705497,
        0.01304860,
        0.02186613,
        -0.00044516,
        0.00443816,
        0.02123462,
        -0.00900067,
        0.02808619,
        -0.00069790,
        0.00723525,
        -0.03541517,
        0.00054277,
        0.00457999,
        0.00391639,
        -0.00836064,
        -0.00862783,
        -0.00347063,
        0.00661578,
        -0.00616864,
        -0.00129618,
        0.01089079,
        -0.00963933,
        -0.00265747,
        -0.00609216,
        -0.01428360,
        -0.00690326,
        0.00598589,
        -0.00141808,
        -0.00766637,
        -0.00563078,
        0.00103317,
        -0.00549794,
        -0.00339958,
        0.01535745,
        -0.00779424,
        -0.00051603,
        -0.00689776,
        0.00672581,
        0.00489062,
        -0.01046298,
        -0.00153764,
        0.01137449,
        0.00019427,
        0.00352505,
        0.01106645,
        -0.00325858,
        -0.01342477,
        0.00084053,
        0.00735775,
        -0.00149757,
        -0.01594285,
        0.00096097,
        -0.00549709,
        0.00603137,
        -0.00027786,
        -0.00243330,
        -0.00095889,
        0.00223883,
        0.00900579,
        0.00107754,
        0.00365070,
        0.00015150,
        0.00153795,
        0.00685195,
        -0.01102705,
        0.01336526,
        0.06330828,
        0.01472186,
        -0.00948722,
        0.00951088,
        -0.02122735,
        -0.00657814,
        0.00736579,
        -0.00494730,
        0.00945349,
        -0.00910751,
        0.00156993,
        -0.01752120,
        -0.00516317,
        -0.00036133,
        0.01299930,
        -0.00960670,
        -0.00695372,
        0.00358371,
        -0.00248066,
        -0.00085553,
        0.01013308,
        -0.01031310,
        0.01391146,
        -0.00500684,
        -0.01070302,
        0.00551785,
        0.01211034,
        -0.00066270,
        -0.00748760,
        0.01321500,
        -0.00914815,
        0.00367207,
        -0.00230517,
        0.00171125,
        -0.00573824,
        -0.00231329,
        0.00798303,
        -0.01103654,
        -0.00069986,
        0.01773706,
        0.00760968,
        -0.00032401,
        -0.00831888,
        0.00282665,
        0.00401237,
        0.00646741,
        0.02859090,
        0.00270779,
        -0.05185343,
        0.01053533,
        -0.00342470,
        -0.00574274,
        -0.00148180,
        -0.00443228,
        -0.00244637,
        0.01041581,
        0.00580057,
        -0.00174600,
        -0.00167422,
        -0.00006874,
        0.00696707,
        0.01696395,
        -0.00887856,
        -0.01404375,
        -0.00735852,
        0.00454126,
        0.00451603,
        -0.00009190,
        -0.00279887,
        0.00881306,
        0.00254559,
        -0.00333110,
        0.00718494,
        -0.00642254,
        -0.00157037,
        0.00406956,
        0.00896032,
        0.00668507,
        -0.00638110,
        0.00457055,
        -0.00124432,
        0.00211392,
        -0.00490214,
        0.00855329,
        -0.01061018,
        0.00374296,
        0.01959687,
        -0.00374546,
        -0.00886619,
        0.00798554,
        -0.00540965,
        -0.00297704,
        0.00608164,
        0.00523561,
        0.01267846,
        -0.00429216,
        -0.01136444,
        0.00498445,
        -0.01758464,
        0.01302850,
        -0.00007140,
        0.01033403,
        0.00269672,
        0.00674951,
        0.00206539,
        -0.00862200,
        0.00393849,
        -0.00504716,
        -0.00120369,
        0.01363795,
        0.00965599,
        -0.01106959,
        0.00534806,
        -0.01509123,
        -0.00450012,
        -0.00187109,
        0.00254361,
        -0.00813596,
        0.00054829,
        0.00250690,
        0.00753453,
    ];

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
        at.update(0, quote!(100.0), quote!(0.0));
        at.update(0, quote!(200.0), quote!(0.0));
        assert_eq!(at.buy_and_hold_return(), quote!(100.0));
    }

    #[test]
    fn acc_tracker_sell_and_hold_return() {
        let mut at = FullAccountTracker::new(quote!(100.0));
        at.update(0, quote!(100.0), quote!(0.0));
        at.update(0, quote!(50.0), quote!(0.0));
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
        acc_tracker.update(0, quote!(100.0), quote!(0.0));
        acc_tracker.update(0, quote!(200.0), quote!(0.0));
        assert_eq!(acc_tracker.buy_and_hold_return(), quote!(100.0));
    }

    #[test]
    fn acc_tracker_sell_and_hold() {
        let mut acc_tracker = FullAccountTracker::new(quote!(100.0));
        acc_tracker.update(0, quote!(100.0), quote!(0.0));
        acc_tracker.update(0, quote!(200.0), quote!(0.0));
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
                acc_tracker.cornish_fisher_value_at_risk(ReturnsSource::Hourly, 0.05),
                3
            ),
            1.354
        );
        assert_eq!(
            round(
                acc_tracker.cornish_fisher_value_at_risk(ReturnsSource::Hourly, 0.01),
                3
            ),
            5.786
        );
    }

    #[test]
    fn acc_tracker_cornish_fisher_value_at_risk_from_n_hourly_returns() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let mut at = FullAccountTracker::new(quote!(100.0));
        at.hist_ln_returns_hourly_acc = LN_RETS_H.into();

        assert_eq!(
            round(
                at.cornish_fisher_value_at_risk_from_n_hourly_returns(24, 0.05),
                3
            ),
            4.043
        );
        assert_eq!(
            round(
                at.cornish_fisher_value_at_risk_from_n_hourly_returns(24, 0.01),
                3
            ),
            5.358
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
