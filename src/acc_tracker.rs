use crate::welford_online::WelfordOnline;
use crate::orders_decimal::Side;


#[derive(Debug, Clone)]
pub struct AccTracker {
    wallet_balance: f64,
    total_rpnl: f64,
    num_trades: i64,
    num_buys: i64,
    total_turnover: f64,
    wb_high: f64,  // wallet balance high
    max_drawdown: f64,
    max_upnl_drawdown: f64,
    welford_returns: WelfordOnline,
    welford_pos_returns: WelfordOnline,
    wins: usize,
    num_submitted_limit_orders: usize,
    num_cancelled_limit_orders: usize,
    num_filled_limit_orders: usize,
}


impl AccTracker {
    pub fn new(starting_wb: f64) -> Self {
        AccTracker{
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
        }
    }

    pub fn sharpe(&self) -> f64 {
        self.total_rpnl / self.welford_returns.std_dev()
    }

    pub fn sortino(&self) -> f64 {
        self.total_rpnl / self.welford_pos_returns.std_dev()
    }

    /// metric that penalizes both std_dev as well as drawdown in returns
    /// see paper: https://arxiv.org/pdf/2008.09471.pdf
    pub fn sharpe_sterling_ratio(&self) -> f64 {
        self.total_rpnl / (self.welford_returns.std_dev() * self.max_drawdown)
    }

    pub fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }

    pub fn num_trades(&self) -> i64 {
        self.num_trades
    }

    pub fn buy_ratio(&self) -> f64 {
        self.num_buys as f64 / self.num_trades as f64
    }

    pub fn turnover(&self) -> f64 {
        self.total_turnover
    }

    pub fn total_rpnl(&self) -> f64 {
        self.total_rpnl
    }

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

    pub fn log_trade(&mut self, side: Side, size: f64, upnl: f64) {
        self.total_turnover += size;
        self.num_trades += 1;
        match side {
            Side::Buy => self.num_buys += 1,
            Side::Sell => {},
        }
        if upnl < self.max_upnl_drawdown {
            self.max_upnl_drawdown = upnl;
        }
    }

    pub fn log_limit_order_submission(&mut self) {
        self.num_submitted_limit_orders += 1;
    }

    pub fn log_limit_order_cancellation(&mut self) {
        self.num_cancelled_limit_orders += 1;
    }

    pub fn log_limit_order_fill(&mut self) {
        self.num_filled_limit_orders += 1;
    }

    pub fn win_ratio(&self) -> f64 {
        self.wins as f64 / self.num_trades as f64
    }

    pub fn limit_order_fill_ratio(&self) -> f64 {
        self.num_filled_limit_orders as f64 / self.num_submitted_limit_orders as f64
    }

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
