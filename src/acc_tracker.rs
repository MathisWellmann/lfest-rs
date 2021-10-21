use crate::cornish_fisher::cornish_fisher_value_at_risk;
use crate::{FuturesTypes, Side};

const DAILY_NS: u64 = 86_400_000_000_000;
const HOURLY_NS: u64 = 3_600_000_000_000;

// TODO: maybe rename this to Stats?

/// Defines the possible sources of returns to use
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ReturnsSource {
    /// Daily sampled returns
    Daily,
    /// Hourly sampled returns
    Hourly,
    /// Tick-by-tick sampled returns
    TickByTick,
}

#[derive(Debug, Clone)]
/// Used for keeping track of account statistics
pub struct AccTracker {
    wallet_balance_last: f64,  // last wallet balance recording
    wallet_balance_start: f64, // wallet balance at start
    wallet_balance_high: f64,  // maximum wallet balance observed
    total_balance_high: f64,   // wallet_balance + upnl high
    futures_type: FuturesTypes,
    total_rpnl: f64,
    upnl: f64,
    num_trades: i64,
    num_buys: i64,
    num_wins: usize,
    num_losses: usize,
    num_submitted_limit_orders: usize,
    num_cancelled_limit_orders: usize,
    num_filled_limit_orders: usize,
    num_trading_opportunities: usize,
    total_turnover: f64,
    max_drawdown_wallet_balance: f64,
    max_drawdown_total: f64,
    // historical daily absolute returns
    hist_returns_daily_acc: Vec<f64>,
    hist_returns_daily_bnh: Vec<f64>,
    // historical hourly absolute returns
    hist_returns_hourly_acc: Vec<f64>,
    hist_returns_hourly_bnh: Vec<f64>,
    // historical tick by tick absolute returns
    // TODO: if these tick-by-tick returns vectors get too large, disable it in live mode
    hist_returns_tick_acc: Vec<f64>,
    hist_returns_tick_bnh: Vec<f64>,
    // historical daily logarithmic returns
    hist_ln_returns_daily_acc: Vec<f64>,
    hist_ln_returns_daily_bnh: Vec<f64>,
    // historical hourly logarithmic returns
    hist_ln_returns_hourly_acc: Vec<f64>,
    hist_ln_returns_hourly_bnh: Vec<f64>,
    // historical tick by tick logarithmic returns
    hist_ln_returns_tick_acc: Vec<f64>,
    hist_ln_returns_tick_bnh: Vec<f64>,
    // timestamps for when to trigger the next pnl snapshots
    next_daily_trigger_ts: u64,
    next_hourly_trigger_ts: u64,
    last_daily_pnl: f64,
    last_hourly_pnl: f64,
    last_tick_pnl: f64,
    cumulative_fees: f64,
    total_profit: f64,
    total_loss: f64,
    price_first: f64,
    price_last: f64,
    price_a_day_ago: f64,
    price_an_hour_ago: f64,
    price_a_tick_ago: f64,
    ts_first: u64,
    ts_last: u64,
}

impl AccTracker {
    #[must_use]
    #[inline]
    /// Create a new AccTracker struct
    pub(crate) fn new(starting_wb: f64, futures_type: FuturesTypes) -> Self {
        AccTracker {
            wallet_balance_last: starting_wb,
            wallet_balance_start: starting_wb,
            wallet_balance_high: starting_wb,
            total_balance_high: starting_wb,
            futures_type,
            total_rpnl: 0.0,
            upnl: 0.0,
            num_trades: 0,
            num_buys: 0,
            num_wins: 0,
            num_losses: 0,
            num_submitted_limit_orders: 0,
            num_cancelled_limit_orders: 0,
            num_filled_limit_orders: 0,
            num_trading_opportunities: 0,
            total_turnover: 0.0,
            max_drawdown_wallet_balance: 0.0,
            max_drawdown_total: 0.0,
            hist_returns_daily_acc: vec![],
            hist_returns_daily_bnh: vec![],
            hist_returns_hourly_acc: vec![],
            hist_returns_hourly_bnh: vec![],
            hist_returns_tick_acc: vec![],
            hist_returns_tick_bnh: vec![],
            hist_ln_returns_daily_acc: vec![],
            hist_ln_returns_daily_bnh: vec![],
            hist_ln_returns_hourly_acc: vec![],
            hist_ln_returns_hourly_bnh: vec![],
            hist_ln_returns_tick_acc: vec![],
            hist_ln_returns_tick_bnh: vec![],
            next_daily_trigger_ts: 0,
            next_hourly_trigger_ts: 0,
            last_daily_pnl: 0.0,
            last_hourly_pnl: 0.0,
            last_tick_pnl: 0.0,
            cumulative_fees: 0.0,
            total_profit: 0.0,
            total_loss: 0.0,
            price_first: 0.0,
            price_last: 0.0,
            price_a_day_ago: 0.0,
            price_an_hour_ago: 0.0,
            price_a_tick_ago: 0.0,
            ts_first: 0,
            ts_last: 0,
        }
    }

    /// Vector of absolute returns the account has generated, including unrealized pnl
    /// # Parameters
    /// source: the sampling interval of pnl snapshots
    #[inline(always)]
    pub fn absolute_returns(&self, source: &ReturnsSource) -> &Vec<f64> {
        match source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
            ReturnsSource::TickByTick => &self.hist_returns_tick_acc,
        }
    }

    /// Vector of natural logarithmic returns the account has generated, including unrealized pnl
    /// # Parameters
    /// source: the sampling interval of pnl snapshots
    #[inline(always)]
    pub fn ln_returns(&self, source: &ReturnsSource) -> &Vec<f64> {
        match source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
            ReturnsSource::TickByTick => &self.hist_ln_returns_tick_acc,
        }
    }

    /// Ratio of cumulative trade profit over cumulative trade loss
    #[inline(always)]
    pub fn profit_loss_ratio(&self) -> f64 {
        self.total_profit / self.total_loss
    }

    /// Cumulative fees paid to the exchange
    #[inline(always)]
    pub fn cumulative_fees(&self) -> f64 {
        self.cumulative_fees
    }

    /// Would be return of buy and hold strategy
    #[inline(always)]
    pub fn buy_and_hold_return(&self) -> f64 {
        let qty = match self.futures_type {
            FuturesTypes::Linear => self.wallet_balance_start / self.price_first,
            FuturesTypes::Inverse => self.wallet_balance_start * self.price_first,
        };
        self.futures_type
            .pnl(self.price_first, self.price_last, qty)
    }

    /// Would be return of sell and hold strategy
    #[inline(always)]
    pub fn sell_and_hold_return(&self) -> f64 {
        let qty = match self.futures_type {
            FuturesTypes::Linear => self.wallet_balance_start / self.price_first,
            FuturesTypes::Inverse => self.wallet_balance_start * self.price_first,
        };
        self.futures_type
            .pnl(self.price_first, self.price_last, -qty)
    }

    /// Return the sharpe ratio using the selected returns as source
    /// # Parameters:
    /// returns_source: the sampling interval of pnl snapshots
    /// risk_free_is_buy_and_hold: if true, it will use the market returns as the risk-free comparison
    ///     else risk-free rate is zero
    pub fn sharpe(&self, returns_source: &ReturnsSource, risk_free_is_buy_and_hold: bool) -> f64 {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
            ReturnsSource::TickByTick => &self.hist_returns_tick_acc,
        };
        if risk_free_is_buy_and_hold {
            let rets_bnh = match returns_source {
                ReturnsSource::Daily => &self.hist_returns_daily_bnh,
                ReturnsSource::Hourly => &self.hist_returns_hourly_bnh,
                ReturnsSource::TickByTick => &self.hist_returns_tick_bnh,
            };
            let n: f64 = rets_acc.len() as f64;
            // compute the difference of returns of account and market
            let diff_returns: Vec<f64> = rets_acc
                .iter()
                .zip(rets_bnh)
                .map(|(a, b)| *a - *b)
                .collect();
            let avg = diff_returns.iter().sum::<f64>() / n;
            let variance = diff_returns.iter().map(|v| (*v - avg).powi(2)).sum::<f64>() / n;
            let std_dev = variance.sqrt();

            (self.total_rpnl - self.buy_and_hold_return()) / std_dev
        } else {
            let n = rets_acc.len() as f64;
            let avg = rets_acc.iter().sum::<f64>() / n;
            let var = rets_acc.iter().map(|v| (*v - avg).powi(2)).sum::<f64>() / n;
            let std_dev = var.sqrt();

            self.total_rpnl / std_dev
        }
    }

    /// Return the Sortino ratio based on daily returns data
    /// # Parameters:
    /// returns_source: the sampling interval of pnl snapshots
    /// risk_free_is_buy_and_hold: if true, it will use the market returns as the risk-free comparison
    ///     else risk-free rate is zero
    pub fn sortino(&self, returns_source: &ReturnsSource, risk_free_is_buy_and_hold: bool) -> f64 {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_returns_hourly_acc,
            ReturnsSource::TickByTick => &self.hist_returns_tick_acc,
        };
        if risk_free_is_buy_and_hold {
            let rets_bnh = match returns_source {
                ReturnsSource::Daily => &self.hist_returns_daily_bnh,
                ReturnsSource::Hourly => &self.hist_returns_hourly_bnh,
                ReturnsSource::TickByTick => &self.hist_returns_tick_bnh,
            };
            // compute the difference of returns of account and market
            let diff_returns: Vec<f64> = rets_acc
                .iter()
                .zip(rets_bnh)
                .map(|(a, b)| *a - *b)
                .filter(|v| *v < 0.0)
                .collect();
            let n: f64 = diff_returns.len() as f64;
            let avg = diff_returns.iter().sum::<f64>() / n;
            let variance = diff_returns.iter().map(|v| (*v - avg).powi(2)).sum::<f64>() / n;
            let std_dev = variance.sqrt();

            (self.total_rpnl - self.buy_and_hold_return()) / std_dev
        } else {
            let downside_rets: Vec<f64> =
                rets_acc.iter().map(|v| *v).filter(|v| *v < 0.0).collect();
            let n = downside_rets.len() as f64;
            let avg = downside_rets.iter().sum::<f64>() / n;
            let var = downside_rets
                .iter()
                .map(|v| (*v - avg).powi(2))
                .sum::<f64>()
                / n;
            let std_dev = var.sqrt();

            self.total_rpnl / std_dev
        }
    }

    /// Calculate the value at risk using the percentile method on daily returns multiplied by starting wallet balance
    /// The time horizon N is assumed to be 1
    /// The literature says if you want a larger N, just multiply by N.sqrt(), which assumes standard normal distribution
    /// # Arguments
    /// returns_source: the sampling interval of pnl snapshots
    /// percentile: value between [0.0, 1.0], smaller value will return more worst case results
    #[inline]
    pub fn historical_value_at_risk(&self, returns_source: &ReturnsSource, percentile: f64) -> f64 {
        let mut rets = match returns_source {
            ReturnsSource::Daily => self.hist_ln_returns_daily_acc.clone(),
            ReturnsSource::Hourly => self.hist_ln_returns_hourly_acc.clone(),
            ReturnsSource::TickByTick => self.hist_ln_returns_tick_acc.clone(),
        };
        quickersort::sort_floats(&mut rets);
        let idx = (rets.len() as f64 * percentile) as usize;
        match rets.get(idx) {
            Some(r) => self.wallet_balance_start - (self.wallet_balance_start * r.exp()),
            None => 0.0,
        }
    }

    /// Calculate the cornish fisher value at risk based on daily returns of the account
    /// # Arguments
    /// returns_source: the sampling interval of pnl snapshots
    /// percentile: in range [0.0, 1.0], usually something like 0.01 or 0.05
    #[inline]
    pub fn cornish_fisher_value_at_risk(
        &self,
        returns_source: &ReturnsSource,
        percentile: f64,
    ) -> f64 {
        let rets = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
            ReturnsSource::TickByTick => &self.hist_ln_returns_tick_acc,
        };
        cornish_fisher_value_at_risk(&rets, self.wallet_balance_start, percentile).2
    }

    /// Return the number of trading days
    #[inline(always)]
    pub fn num_trading_days(&self) -> u64 {
        (self.ts_last - self.ts_first) / DAILY_NS
    }

    /// Also called discriminant-ratio, which focuses on the added value of the algorithm
    /// It uses the Cornish-Fish Value at Risk (CF-VaR)
    /// It better captures the risk of the asset as it is not limited by the assumption of a gaussian distribution
    /// It it time-insensitive
    /// from: https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3927058
    /// # Parameters
    /// returns_source: the sampling interval of pnl snapshots
    pub fn d_ratio(&self, returns_source: &ReturnsSource) -> f64 {
        let rets_acc = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_acc,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_acc,
            ReturnsSource::TickByTick => &self.hist_ln_returns_tick_acc,
        };
        let rets_bnh = match returns_source {
            ReturnsSource::Daily => &self.hist_ln_returns_daily_bnh,
            ReturnsSource::Hourly => &self.hist_ln_returns_hourly_bnh,
            ReturnsSource::TickByTick => &self.hist_ln_returns_tick_bnh,
        };

        let cf_var_bnh = cornish_fisher_value_at_risk(rets_bnh, self.wallet_balance_start, 0.01).1;
        let cf_var_acc = cornish_fisher_value_at_risk(rets_acc, self.wallet_balance_start, 0.01).1;

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
        info!(
            "roi_acc: {:.2}, roi_bnh: {:.2}, cf_var_bnh: {:.8}, cf_var_acc: {:.8}, rtv_acc: {}, rtv_bnh: {}",
            roi_acc, roi_bnh, cf_var_bnh, cf_var_acc, rtv_acc, rtv_bnh,
        );

        (1.0 + (roi_acc - roi_bnh) / roi_bnh.abs()) * (cf_var_bnh / cf_var_acc)
    }

    /// Annualized return on investment as a factor, e.g.: 100% -> 2x
    pub fn annualized_roi(&self) -> f64 {
        (1.0 + (self.total_rpnl / self.wallet_balance_start))
            .powf(365.0 / self.num_trading_days() as f64)
    }

    /// Maximum drawdown of the wallet balance
    #[inline(always)]
    pub fn max_drawdown_wallet_balance(&self) -> f64 {
        self.max_drawdown_wallet_balance
    }

    /// Maximum drawdown of the wallet balance including unrealized profit and loss
    #[inline(always)]
    pub fn max_drawdown_total(&self) -> f64 {
        self.max_drawdown_total
    }

    /// Return the number of trades the account made
    #[inline(always)]
    pub fn num_trades(&self) -> i64 {
        self.num_trades
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
    pub fn turnover(&self) -> f64 {
        self.total_turnover
    }

    /// Return the total realized profit and loss of the account
    #[inline(always)]
    pub fn total_rpnl(&self) -> f64 {
        self.total_rpnl
    }

    /// Return the current unrealized profit and loss
    #[inline(always)]
    pub fn upnl(&self) -> f64 {
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

    /// Return the ratio of filled limit orders vs number of submitted limit orders
    #[inline(always)]
    pub fn limit_order_fill_ratio(&self) -> f64 {
        self.num_filled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

    /// Return the ratio of limit order cancellations vs number of submitted limit orders
    #[inline(always)]
    pub fn limit_order_cancellation_ratio(&self) -> f64 {
        self.num_cancelled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

    /// Log the realized profit and loss of a trade
    pub(crate) fn log_rpnl(&mut self, rpnl: f64) {
        self.total_rpnl += rpnl;
        self.wallet_balance_last += rpnl;
        if rpnl < 0.0 {
            self.total_loss += rpnl.abs();
            self.num_losses += 1;
        } else {
            self.num_wins += 1;
            self.total_profit += rpnl;
        }
        if self.wallet_balance_last > self.wallet_balance_high {
            self.wallet_balance_high = self.wallet_balance_last;
        }
        let dd = (self.wallet_balance_high - self.wallet_balance_last) / self.wallet_balance_high;
        if dd > self.max_drawdown_wallet_balance {
            self.max_drawdown_wallet_balance = dd;
        }
    }

    /// Log a user trade
    #[inline]
    pub(crate) fn log_trade(&mut self, side: Side, size: f64, price: f64) {
        self.total_turnover += match self.futures_type {
            FuturesTypes::Linear => size * price,
            FuturesTypes::Inverse => size / price,
        };
        self.num_trades += 1;
        match side {
            Side::Buy => self.num_buys += 1,
            Side::Sell => {}
        }
    }

    /// Update the most recent timestamp which is used for daily rpnl calculation.
    /// Assumes timestamp in nanoseconds
    pub(crate) fn update(&mut self, ts: u64, price: f64, upnl: f64) {
        self.price_last = price;
        if self.price_a_day_ago == 0.0 {
            self.price_a_day_ago = price;
        }
        if self.price_an_hour_ago == 0.0 {
            self.price_an_hour_ago = price;
        }
        if self.price_a_tick_ago == 0.0 {
            self.price_a_tick_ago = price;
        }
        if self.price_first == 0.0 {
            self.price_first = price;
        }
        self.num_trading_opportunities += 1;
        if self.ts_first == 0 {
            self.ts_first = ts;
        }
        self.ts_last = ts;
        if ts > self.next_daily_trigger_ts {
            self.next_daily_trigger_ts = ts + DAILY_NS;

            // calculate daily return of account
            let pnl: f64 = (self.total_rpnl + upnl) - self.last_daily_pnl;
            self.hist_returns_daily_acc.push(pnl);

            // calculate daily log return of account
            let ln_ret: f64 = ((self.wallet_balance_last + upnl)
                / (self.wallet_balance_start + self.last_daily_pnl))
                .ln();
            self.hist_ln_returns_daily_acc.push(ln_ret);

            // calculate daily return of buy_and_hold
            let bnh_qty = self.wallet_balance_start / self.price_first;
            let pnl_bnh = self.futures_type.pnl(self.price_a_day_ago, price, bnh_qty);
            self.hist_returns_daily_bnh.push(pnl_bnh);

            // calculate daily log return of market
            let ln_ret: f64 = (price / self.price_a_day_ago).ln();
            self.hist_ln_returns_daily_bnh.push(ln_ret);

            self.last_daily_pnl = self.total_rpnl + upnl;
            self.price_a_day_ago = price;
        }
        if ts > self.next_hourly_trigger_ts {
            self.next_hourly_trigger_ts = ts + HOURLY_NS;

            // calculate hourly return of account
            let pnl: f64 = (self.total_rpnl + upnl) - self.last_hourly_pnl;
            self.hist_returns_hourly_acc.push(pnl);

            // calculate hourly logarithmic return of account
            let ln_ret: f64 = ((self.wallet_balance_last + upnl)
                / (self.wallet_balance_start + self.last_hourly_pnl))
                .ln();
            self.hist_ln_returns_hourly_acc.push(ln_ret);

            // calculate hourly return of buy_and_hold
            let bnh_qty = self.wallet_balance_start / self.price_first;
            let pnl_bnh = self
                .futures_type
                .pnl(self.price_an_hour_ago, price, bnh_qty);
            self.hist_returns_hourly_bnh.push(pnl_bnh);

            // calculate hourly logarithmic return of buy_and_hold
            let ln_ret: f64 = (price / self.price_an_hour_ago).ln();
            self.hist_ln_returns_hourly_bnh.push(ln_ret);

            self.last_hourly_pnl = self.total_rpnl + upnl;
            self.price_an_hour_ago = price;
        }
        // compute tick-by-tick return statistics
        let pnl: f64 = (self.total_rpnl + upnl) - self.last_tick_pnl;
        self.hist_returns_tick_acc.push(pnl);

        let ln_ret: f64 = ((self.wallet_balance_last + upnl)
            / (self.wallet_balance_start + self.last_tick_pnl))
            .ln();
        self.hist_ln_returns_tick_acc.push(ln_ret);

        let bnh_qty = self.wallet_balance_start / self.price_first;
        let pnl_bnh: f64 = self.futures_type.pnl(self.price_a_tick_ago, price, bnh_qty);
        self.hist_returns_tick_bnh.push(pnl_bnh);

        let ln_ret = (price / self.price_a_tick_ago).ln();
        self.hist_ln_returns_tick_bnh.push(ln_ret);

        self.last_tick_pnl = self.total_rpnl + upnl;
        self.price_a_tick_ago = price;

        // update max_drawdown_total
        let curr_dd = (self.wallet_balance_high - (self.wallet_balance_last + upnl))
            / self.wallet_balance_high;
        if curr_dd > self.max_drawdown_total {
            self.max_drawdown_total = curr_dd;
        }
    }

    /// Update the cumulative fee amount
    #[inline(always)]
    pub(crate) fn log_fee(&mut self, fee: f64) {
        self.cumulative_fees += fee
    }

    /// Log a limit order submission
    #[inline(always)]
    pub(crate) fn log_limit_order_submission(&mut self) {
        self.num_submitted_limit_orders += 1;
    }

    /// Log a limit order cancellation
    #[inline(always)]
    pub(crate) fn log_limit_order_cancellation(&mut self) {
        self.num_cancelled_limit_orders += 1;
    }

    /// Log a limit order fill
    #[inline(always)]
    pub(crate) fn log_limit_order_fill(&mut self) {
        self.num_filled_limit_orders += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::round;

    #[test]
    fn acc_tracker_profit_loss_ratio() {
        let mut at = AccTracker::new(100.0, FuturesTypes::Linear);
        at.total_profit = 50.0;
        at.total_loss = 25.0;
        assert_eq!(at.profit_loss_ratio(), 2.0);
    }

    #[test]
    fn acc_tracker_cumulative_fees() {
        let mut at = AccTracker::new(100.0, FuturesTypes::Linear);
        at.log_fee(0.1);
        at.log_fee(0.2);
        assert_eq!(round(at.cumulative_fees(), 1), 0.3);
    }

    #[test]
    fn acc_tracker_buy_and_hold_return() {
        let mut at = AccTracker::new(100.0, FuturesTypes::Linear);
        at.update(0, 100.0, 0.0);
        at.update(0, 200.0, 0.0);
        assert_eq!(at.buy_and_hold_return(), 100.0);
    }

    #[test]
    fn acc_tracker_sell_and_hold_return() {
        let mut at = AccTracker::new(100.0, FuturesTypes::Linear);
        at.update(0, 100.0, 0.0);
        at.update(0, 50.0, 0.0);
        assert_eq!(at.sell_and_hold_return(), 50.0);
    }

    #[test]
    fn acc_tracker_log_rpnl() {
        let rpnls: Vec<f64> = vec![0.1, -0.1, 0.1, 0.2, -0.1];
        let mut acc_tracker = AccTracker::new(1.0, FuturesTypes::Linear);
        for r in rpnls {
            acc_tracker.log_rpnl(r);
        }

        assert_eq!(round(acc_tracker.max_drawdown_wallet_balance(), 2), 0.09);
        assert_eq!(round(acc_tracker.total_rpnl(), 1), 0.20);
    }

    #[test]
    fn acc_tracker_buy_and_hold() {
        let mut acc_tracker = AccTracker::new(100.0, FuturesTypes::Linear);
        acc_tracker.update(0, 100.0, 0.0);
        acc_tracker.update(0, 200.0, 0.0);
        assert_eq!(acc_tracker.buy_and_hold_return(), 100.0);
    }

    #[test]
    fn acc_tracker_sell_and_hold() {
        let mut acc_tracker = AccTracker::new(100.0, FuturesTypes::Linear);
        acc_tracker.update(0, 100.0, 0.0);
        acc_tracker.update(0, 200.0, 0.0);
        assert_eq!(acc_tracker.sell_and_hold_return(), -100.0);
    }

    #[test]
    fn acc_tracker_value_at_risk_percentile() {
        if let Err(_e) = pretty_env_logger::try_init() {}

        let mut acc_tracker = AccTracker::new(100.0, FuturesTypes::Linear);
        let daily_returns = vec![0.05, -0.05, 0.0, 0.1, -0.1, -0.025, 0.25, 0.03, -0.03, 0.0];
        acc_tracker.hist_ln_returns_daily_acc = daily_returns;

        assert_eq!(
            round(
                acc_tracker.historical_value_at_risk(&ReturnsSource::Daily, 0.05),
                3
            ),
            9.516
        );
    }
}
