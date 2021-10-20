use crate::cornish_fisher::cornish_fisher_value_at_risk;
use crate::welford_online::WelfordOnline;
use crate::{FuturesTypes, Side};

const DAILY_NS: u64 = 86_400_000_000_000;

// TODO: maybe rename this to Stats?

#[derive(Debug, Clone)]
/// Used for keeping track of account statistics
pub struct AccTracker {
    wallet_balance: f64,
    futures_type: FuturesTypes,
    starting_wb: f64,
    total_rpnl: f64,
    upnl: f64,
    num_trades: i64,
    num_buys: i64,
    total_turnover: f64,
    wb_high: f64, // wallet balance high
    max_drawdown: f64,
    max_upnl_drawdown: f64,
    welford_returns: WelfordOnline,
    welford_neg_returns: WelfordOnline,
    wins: usize,
    losses: usize,
    num_submitted_limit_orders: usize,
    num_cancelled_limit_orders: usize,
    num_filled_limit_orders: usize,
    // log returns of daily account balance
    returns_daily_account: Vec<f64>,
    // log returns of daily buy and hold strategy
    returns_daily_bnh: Vec<f64>,
    trade_returns: Vec<f64>,
    next_trigger_ts: u64,
    last_rpnl_entry: f64,
    cumulative_fees: f64,
    num_trading_opportunities: usize,
    total_profit: f64,
    total_loss: f64,
    win_history: Vec<bool>, // history of all wins and losses. true is a win, false is a loss
    first_price: f64,
    last_price: f64,
    price_a_day_ago: f64,
    first_ts: u64,
    last_ts: u64,
}

impl AccTracker {
    #[must_use]
    #[inline]
    pub fn new(starting_wb: f64, futures_type: FuturesTypes) -> Self {
        AccTracker {
            wallet_balance: starting_wb,
            futures_type,
            starting_wb,
            total_rpnl: 0.0,
            upnl: 0.0,
            num_trades: 0,
            num_buys: 0,
            total_turnover: 0.0,
            wb_high: starting_wb,
            max_drawdown: 0.0,
            max_upnl_drawdown: 0.0,
            welford_returns: WelfordOnline::new(),
            welford_neg_returns: WelfordOnline::new(),
            wins: 0,
            losses: 0,
            num_submitted_limit_orders: 0,
            num_cancelled_limit_orders: 0,
            num_filled_limit_orders: 0,
            returns_daily_account: vec![],
            returns_daily_bnh: vec![],
            trade_returns: vec![],
            next_trigger_ts: 0,
            last_rpnl_entry: 0.0,
            cumulative_fees: 0.0,
            num_trading_opportunities: 0,
            total_profit: 0.0,
            total_loss: 0.0,
            win_history: vec![],
            first_price: 0.0,
            last_price: 0.0,
            price_a_day_ago: 0.0,
            first_ts: 0,
            last_ts: 0,
        }
    }

    /// Return the history of wins and losses, where true is a win, false is a loss
    #[inline(always)]
    pub fn win_history(&self) -> &Vec<bool> {
        &self.win_history
    }

    /// Return the ratio of average trade profit over average trade loss
    #[inline(always)]
    pub fn profit_loss_ratio(&self) -> f64 {
        self.total_profit / self.total_loss
    }

    /// Return the cumulative fees paid to the exchange
    #[inline(always)]
    pub fn cumulative_fees(&self) -> f64 {
        self.cumulative_fees
    }

    #[inline(always)]
    pub fn buy_and_hold_return(&self) -> f64 {
        self.futures_type.pnl(
            self.first_price,
            self.last_price,
            self.starting_wb / self.first_price,
        )
    }

    #[inline(always)]
    pub fn sell_and_hold_return(&self) -> f64 {
        self.futures_type.pnl(
            self.first_price,
            self.last_price,
            -self.starting_wb / self.first_price,
        )
    }

    /// Return the sharpe ratio based on individual trade data
    /// risk adjusted return is the excess return over buy and hold
    #[inline(always)]
    pub fn sharpe(&self) -> f64 {
        (self.total_rpnl - self.buy_and_hold_return()) / self.welford_returns.std_dev()
    }

    /// Return the sharpe ratio based on daily returns
    /// risk adjusted return is the excess return over buy and hold
    pub fn sharpe_daily_returns(&self) -> f64 {
        let n: f64 = self.returns_daily_account.len() as f64;
        let avg: f64 = self.returns_daily_account.iter().sum::<f64>() / n;
        let variance: f64 = (1.0 / n)
            * self
                .returns_daily_account
                .iter()
                .map(|v| (*v - avg).powi(2))
                .sum::<f64>();
        let std_dev: f64 = variance.sqrt();
        (self.total_rpnl - self.buy_and_hold_return()) / std_dev
    }

    /// Return the Sortino ratio based on individual trade data
    /// risk adjusted reutrn is the excess return over buy and hold
    #[inline(always)]
    pub fn sortino(&self) -> f64 {
        (self.total_rpnl - self.buy_and_hold_return()) / self.welford_neg_returns.std_dev()
    }

    /// Return the Sortino ratio based on daily returns data
    /// risk adjusted reutrn is the excess return over buy and hold
    pub fn sortino_daily_returns(&self) -> f64 {
        let n: f64 = self.returns_daily_account.len() as f64;
        let avg: f64 = self.returns_daily_account.iter().sum::<f64>() / n;
        let variance: f64 = (1.0 / n)
            * self
                .returns_daily_account
                .iter()
                .map(|v| (*v - avg).powi(2))
                .filter(|v| *v < 0.0)
                .sum::<f64>();
        let std_dev: f64 = variance.sqrt();
        (self.total_rpnl - self.buy_and_hold_return()) / std_dev
    }

    /// Calculate the value at risk using the percentile method on daily returns multiplied by starting wallet balance
    /// The time horizon N is assumed to be 1
    /// The literature says if you want a larger N, just multiply by N.sqrt(), which assumes standard normal distribution
    /// # Arguments
    /// percentile: value between [0.0, 1.0], smaller value will return more worst case results
    #[inline]
    pub fn historical_value_at_risk_daily_returns(&self, percentile: f64) -> f64 {
        let mut rets = self.returns_daily_account.clone();
        quickersort::sort_floats(&mut rets);
        let idx = (rets.len() as f64 * percentile) as usize;
        match rets.get(idx) {
            Some(r) => self.starting_wb * (1.0 - r.exp()),
            None => 0.0,
        }
    }

    /// Calculate the value at risk using the percentile method on a trade by trade basis multiplied by starting wallet balance
    /// The time horizon N is assumed to be 1
    /// The literature says if you want a larger N, just multiply by N.sqrt(), which assumes standard normal distribution
    /// # Arguments
    /// percentile: value between [0.0, 1.0], smaller value will return more worst case results
    #[inline]
    pub fn historical_value_at_risk_trade_returns(&self, percentile: f64) -> f64 {
        let mut rets = self.trade_returns.clone();
        quickersort::sort_floats(&mut rets);
        let idx = (rets.len() as f64 * percentile) as usize;
        match rets.get(idx) {
            Some(r) => self.starting_wb * (1.0 - r.exp()),
            None => 0.0,
        }
    }

    /// Calculate the cornish fisher value at risk based on daily returns of the account
    /// # Arguments
    /// percentile: in range [0.0, 1.0], usually something like 0.01 or 0.05
    pub fn cornish_fisher_value_at_risk_daily_returns(&self, percentile: f64) -> f64 {
        cornish_fisher_value_at_risk(&self.returns_daily_account, self.last_price, percentile).1
    }

    /// Calculate the cornish fisher value at risk based on returns of the accounts trades
    /// # Arguments
    /// percentile: in range [0.0, 1.0], usually something like 0.01 or 0.05
    pub fn cornish_fisher_value_at_risk_trade_returns(&self, percentile: f64) -> f64 {
        cornish_fisher_value_at_risk(&self.trade_returns, self.last_price, percentile).1
    }

    /// Return the number of trading days
    #[inline(always)]
    pub fn num_trading_days(&self) -> u64 {
        (self.last_ts - self.first_ts) / DAILY_NS
    }

    /// Also called discriminant-ratio, which focuses on the added value of the algorithm
    /// It uses the Cornish-Fish Value at Risk (CF-VaR)
    /// It better captures the risk of the asset as it is not limited by the assumption of a gaussian distribution
    /// It it time-insensitive
    /// from: https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3927058
    pub fn d_ratio(&self) -> f64 {
        info!(
            "returns_daily_bnh: {:?}, returns_daily_account: {:?}",
            self.returns_daily_bnh, self.returns_daily_account
        );

        let (_, cf_var_bnh, _) =
            cornish_fisher_value_at_risk(&self.returns_daily_bnh, self.last_price, 0.01);
        let (_, cf_var_acc, _) =
            cornish_fisher_value_at_risk(&self.returns_daily_account, self.last_price, 0.01);

        // compute annualized return of account and buy_and_hold strategy
        let num_trading_days = self.num_trading_days() as f64;
        let mean_daily_return_acc = self.returns_daily_account.iter().sum::<f64>()
            / self.returns_daily_account.len() as f64;
        let roi_acc = (1.0 + mean_daily_return_acc) * 365.0 / num_trading_days;

        let mean_daily_return_bnh =
            self.returns_daily_bnh.iter().sum::<f64>() / self.returns_daily_bnh.len() as f64;
        let roi_bnh = (1.0 + mean_daily_return_bnh) * (365.0 / num_trading_days);

        info!(
            "mean_return_acc: {}, mean_return_bnh: {}, roi_acc: {}, roi_bnh: {}, cf_var_bnh: {}, cf_var_acc: {}",
            mean_daily_return_acc,
            mean_daily_return_bnh,
            roi_acc,
            roi_bnh,
            cf_var_bnh,
            cf_var_acc,
        );
        let rtv_acc = roi_acc / cf_var_acc;
        let rtv_bnh = roi_bnh / cf_var_bnh;

        1.0 + (rtv_acc - rtv_bnh) / rtv_bnh.abs()
    }

    /// Return the standard deviation of realized profit and loss returns
    #[inline(always)]
    pub fn std_dev_returns(&self) -> f64 {
        self.welford_returns.std_dev()
    }

    /// Return the standard deviation of negative realized profit and loss returns
    #[inline(always)]
    pub fn std_dev_neg_returns(&self) -> f64 {
        self.welford_neg_returns.std_dev()
    }

    /// metric that penalizes both std_dev as well as drawdown in returns
    /// see paper: https://arxiv.org/pdf/2008.09471.pdf
    #[inline]
    pub fn sharpe_sterling_ratio(&self) -> f64 {
        let mut std_dev = self.welford_returns.std_dev();
        if std_dev < 0.1 {
            // limit the std_dev to 0.1 minimum
            std_dev = 0.1;
        }
        self.total_rpnl / (std_dev * self.max_upnl_drawdown())
    }

    /// Return the maximum drawdown of the realized profit and loss curve
    #[inline(always)]
    pub fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }

    /// Return the maximum drawdown of the unrealized profit and loss curve
    #[inline(always)]
    pub fn max_upnl_drawdown(&self) -> f64 {
        self.max_upnl_drawdown / self.starting_wb
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
        if self.wins + self.losses > 0 {
            self.wins as f64 / (self.wins + self.losses) as f64
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
        self.wallet_balance += rpnl;
        self.welford_returns.add(rpnl);
        if rpnl < 0.0 {
            self.welford_neg_returns.add(rpnl);
            self.total_loss += rpnl.abs();
            self.losses += 1;
            self.win_history.push(false);
        } else {
            self.wins += 1;
            self.total_profit += rpnl;
            self.win_history.push(true);
        }
        if self.wallet_balance > self.wb_high {
            self.wb_high = self.wallet_balance;
        }
        let dd = (self.wb_high - self.wallet_balance) / self.wb_high;
        if dd > self.max_drawdown {
            self.max_drawdown = dd;
        }
        self.trade_returns.push(rpnl);
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

    /// Log the unrealized profit and loss at each new candle or trade
    #[inline]
    pub(crate) fn log_upnl(&mut self, upnl: f64) {
        let upnl_dd: f64 = upnl.abs();
        self.upnl = upnl;
        if upnl_dd > self.max_upnl_drawdown {
            self.max_upnl_drawdown = upnl_dd;
        }
    }

    /// Update the most recent timestamp which is used for daily rpnl calculation.
    /// Assumes timestamp in nanoseconds
    pub(crate) fn update(&mut self, ts: u64, price: f64) {
        self.last_price = price;
        if self.price_a_day_ago == 0.0 {
            self.price_a_day_ago = price;
        }
        if ts > self.next_trigger_ts {
            self.next_trigger_ts = ts + DAILY_NS;
            // calculate daily log return of account
            let mut rpnl: f64 = (self.total_rpnl / self.last_rpnl_entry).ln();
            if !rpnl.is_finite() {
                rpnl = 0.0;
            }
            self.last_rpnl_entry = self.total_rpnl;
            self.returns_daily_account.push(rpnl);

            // calculate daily log return of buy and hold strategy
            let bnh_return: f64 = (self.last_price / self.price_a_day_ago).ln();
            self.returns_daily_bnh.push(bnh_return);
            self.price_a_day_ago = price;
        }
        self.num_trading_opportunities += 1;
        if self.first_price == 0.0 {
            self.first_price = price;
        }
        if self.first_ts == 0 {
            self.first_ts = ts;
        }
        self.last_ts = ts;
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
    fn acc_tracker_log_rpnl() {
        let rpnls: Vec<f64> = vec![0.1, -0.1, 0.1, 0.2, -0.1];
        let mut acc_tracker = AccTracker::new(1.0, FuturesTypes::Linear);
        for r in rpnls {
            acc_tracker.log_rpnl(r);
        }

        assert_eq!(round(acc_tracker.max_drawdown(), 2), 0.09);
        assert_eq!(round(acc_tracker.total_rpnl(), 1), 0.20);
        assert_eq!(round(acc_tracker.welford_returns.std_dev(), 3), 0.134);
        assert_eq!(round(acc_tracker.welford_neg_returns.std_dev(), 3), 0.0);
    }

    #[test]
    fn acc_tracker_buy_and_hold() {
        let mut acc_tracker = AccTracker::new(100.0, FuturesTypes::Linear);
        acc_tracker.update(0, 100.0);
        acc_tracker.update(0, 200.0);
        assert_eq!(acc_tracker.buy_and_hold_return(), 100.0);
    }

    #[test]
    fn acc_tracker_sell_and_hold() {
        let mut acc_tracker = AccTracker::new(100.0, FuturesTypes::Linear);
        acc_tracker.update(0, 100.0);
        acc_tracker.update(0, 200.0);
        assert_eq!(acc_tracker.sell_and_hold_return(), -100.0);
    }

    #[test]
    fn acc_tracker_value_at_risk_percentile() {
        if let Err(_e) = pretty_env_logger::try_init() {}

        let mut acc_tracker = AccTracker::new(100.0, FuturesTypes::Linear);
        let daily_returns = vec![1.0, 2.0, -3.0, -1.0, 3.0, 2.0, 2.0, 1.0, -2.0, -1.0];
        acc_tracker.returns_daily_account = daily_returns;

        assert_eq!(
            acc_tracker.historical_value_at_risk_daily_returns(0.15),
            -2.0
        );
    }
}
