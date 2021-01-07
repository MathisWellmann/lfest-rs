use crate::welford_online::WelfordOnline;
use crate::Side;

const DAILY_MS: u64 = 86_400_000;

#[derive(Debug, Clone)]
pub struct AccTracker {
    wallet_balance: f64,
    total_rpnl: f64,
    num_trades: i64,
    num_buys: i64,
    total_turnover: f64,
    wb_high: f64, // wallet balance high
    max_drawdown: f64,
    max_upnl_drawdown: f64,
    welford_returns: WelfordOnline,
    welford_pos_returns: WelfordOnline,
    wins: usize,
    num_submitted_limit_orders: usize,
    num_cancelled_limit_orders: usize,
    num_filled_limit_orders: usize,
    daily_returns: Vec<f64>,
    next_trigger_ts: u64,
    last_rpnl_entry: f64,
    cumulative_fees: f64,
}

impl AccTracker {
    pub fn new(starting_wb: f64) -> Self {
        AccTracker {
            wallet_balance: starting_wb,
            total_rpnl: 0.0,
            num_trades: 0,
            num_buys: 0,
            total_turnover: 0.0,
            wb_high: starting_wb,
            max_drawdown: 0.0,
            max_upnl_drawdown: 0.0,
            welford_returns: WelfordOnline::new(),
            welford_pos_returns: WelfordOnline::new(),
            wins: 0,
            num_submitted_limit_orders: 0,
            num_cancelled_limit_orders: 0,
            num_filled_limit_orders: 0,
            daily_returns: vec![],
            next_trigger_ts: 0,
            last_rpnl_entry: 0.0,
            cumulative_fees: 0.0,
        }
    }

    /// Return the cumulative fees paid to the exchange denoted in BASE currency
    pub fn cumulative_fees(&self) -> f64 {
        self.cumulative_fees
    }

    /// Return the sharpe ration based on individual trade data
    pub fn sharpe(&self) -> f64 {
        self.total_rpnl / self.welford_returns.std_dev()
    }

    pub fn sharpe_daily_returns(&self) -> f64 {
        let n: f64 = self.daily_returns.len() as f64;
        let avg: f64 = self.daily_returns.iter().sum::<f64>() / n;
        let variance: f64 = (1.0 / n)
            * self
                .daily_returns
                .iter()
                .map(|v| (*v - avg).powi(2))
                .sum::<f64>();
        let std_dev: f64 = variance.sqrt();
        self.total_rpnl / std_dev
    }

    /// Return the Sortino ratio based on individual trade data
    pub fn sortino(&self) -> f64 {
        self.total_rpnl / self.welford_pos_returns.std_dev()
    }

    /// metric that penalizes both std_dev as well as drawdown in returns
    /// see paper: https://arxiv.org/pdf/2008.09471.pdf
    pub fn sharpe_sterling_ratio(&self) -> f64 {
        self.total_rpnl / (self.welford_returns.std_dev() * self.max_drawdown)
    }

    /// Return the maximum drawdown of the realized profit and loss curve
    pub fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }

    /// Return the maximum drawdown of the unrealized profit and loss curve
    pub fn max_upnl_drawdown(&self) -> f64 {
        self.max_upnl_drawdown
    }

    /// Return the number of trades the account made
    pub fn num_trades(&self) -> i64 {
        self.num_trades
    }

    /// Return the ratio of buy trades vs total number of trades
    pub fn buy_ratio(&self) -> f64 {
        self.num_buys as f64 / self.num_trades as f64
    }

    /// Return the cumulative turnover value of the trades, measured in QUOTE currency
    pub fn turnover(&self) -> f64 {
        self.total_turnover
    }

    /// Return the total realized profit and loss of the account
    pub fn total_rpnl(&self) -> f64 {
        self.total_rpnl
    }

    /// Log the realized profit and loss of a trade
    pub fn log_rpnl(&mut self, rpnl: f64) {
        self.total_rpnl += rpnl;
        self.wallet_balance += rpnl;
        self.welford_returns.add(rpnl);
        if rpnl > 0.0 {
            self.welford_pos_returns.add(rpnl);
            self.wins += 1;
        }
        if self.wallet_balance > self.wb_high {
            self.wb_high = self.wallet_balance;
        }
        let dd = (self.wb_high - self.wallet_balance) / self.wb_high;
        if dd > self.max_drawdown {
            self.max_drawdown = dd;
        }
    }

    /// Log the trade
    pub fn log_trade(&mut self, side: Side, size: f64, upnl: f64) {
        self.total_turnover += size;
        self.num_trades += 1;
        match side {
            Side::Buy => self.num_buys += 1,
            Side::Sell => {}
        }
        if upnl < self.max_upnl_drawdown {
            self.max_upnl_drawdown = upnl;
        }
    }

    /// Update the most recent timestamp which is used for daily rpnl calculation.
    /// Assumes timestamp in milliseconds
    pub fn log_timestamp(&mut self, ts: u64) {
        if ts > self.next_trigger_ts {
            self.next_trigger_ts = ts + DAILY_MS;
            // calculate daily rpnl
            let rpnl: f64 = self.total_rpnl - self.last_rpnl_entry;
            self.last_rpnl_entry = self.total_rpnl;
            self.daily_returns.push(rpnl);
        }
    }

    /// Update the cumulative fee amount denoted in BASE currency
    pub fn log_fee(&mut self, fee_base: f64) {
        self.cumulative_fees += fee_base
    }

    /// Log a limit order submission
    pub fn log_limit_order_submission(&mut self) {
        self.num_submitted_limit_orders += 1;
    }

    /// Log a limit order cancellation
    pub fn log_limit_order_cancellation(&mut self) {
        self.num_cancelled_limit_orders += 1;
    }

    /// Log a limit order fill
    pub fn log_limit_order_fill(&mut self) {
        self.num_filled_limit_orders += 1;
    }

    /// Return the ratio of winning trades vs all trades
    pub fn win_ratio(&self) -> f64 {
        self.wins as f64 / self.num_trades as f64
    }

    /// Return the ratio of filled limit orders vs number of submitted limit orders
    pub fn limit_order_fill_ratio(&self) -> f64 {
        self.num_filled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

    /// Return the ratio of limit order cancellations vs number of submitted limit orders
    pub fn limit_order_cancellation_ratio(&self) -> f64 {
        self.num_cancelled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(val: f64, prec: i32) -> f64 {
        ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
    }

    #[test]
    fn log_trade() {
        let trades: Vec<(Side, f64)> = vec![
            (Side::Buy, 1.0),
            (Side::Sell, 1.0),
            (Side::Buy, 1.0),
            (Side::Sell, 1.0),
        ];
        let mut acc_tracker = AccTracker::new(1.0);
        for t in trades {
            acc_tracker.log_trade(t.0, t.1, 0.0);
        }

        assert_eq!(acc_tracker.turnover(), 4.0);
        assert_eq!(acc_tracker.num_trades(), 4);
        assert_eq!(acc_tracker.num_buys, 2);
        assert_eq!(acc_tracker.buy_ratio(), 0.5);
    }

    #[test]
    fn log_rpnl() {
        let rpnls: Vec<f64> = vec![0.1, -0.1, 0.1, 0.2, -0.1];
        let mut acc_tracker = AccTracker::new(1.0);
        for r in rpnls {
            acc_tracker.log_rpnl(r);
        }

        assert_eq!(round(acc_tracker.max_drawdown(), 2), 0.09);
        assert_eq!(round(acc_tracker.total_rpnl(), 1), 0.20);
        assert_eq!(round(acc_tracker.welford_returns.std_dev(), 3), 0.134);
        assert_eq!(round(acc_tracker.welford_pos_returns.std_dev(), 3), 0.058);
        assert_eq!(round(acc_tracker.sharpe(), 3), 1.491);
        assert_eq!(round(acc_tracker.sortino(), 3), 3.464);
    }
}
