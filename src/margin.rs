#[derive(Debug, Clone, Copy)]
/// Describes the margin information of the account
pub struct Margin {
    /// The wallet balance of account denoted in BASE currency
    pub wallet_balance: f64,
    /// The margin balance of account denoted in BASE currency
    pub margin_balance: f64,
    /// The position margin of account denoted in BASE currency
    pub position_margin: f64,
    /// The order margin of account denoted in BASE currency
    pub order_margin: f64,
    /// The available balance of account denoted in BASE currency
    pub available_balance: f64,
}

impl Margin {
    /// Create a new margin account with an initial balance denoted in BASE currency
    pub fn new(init_balance: f64) -> Self {
        Margin {
            wallet_balance: init_balance,
            margin_balance: init_balance,
            position_margin: 0.0,
            order_margin: 0.0,
            available_balance: init_balance,
        }
    }

    /// Reserve some margin for open orders, order_margin denoted in BASE currency
    /// Returns true if successful
    pub fn reserve_order_margin(&mut self, order_margin: f64) -> bool {
        if order_margin > self.available_balance {
            return false;
        }
        self.order_margin += order_margin;
        self.available_balance -= order_margin;

        true
    }

    /// Free some reserved order margin for some order value, denoted in BASE currency
    /// Returns true if successful
    pub fn free_order_margin(&mut self, order_margin: f64) -> bool {
        if order_margin > self.order_margin {
            return false;
        }
        self.order_margin -= order_margin;
        self.available_balance += order_margin;

        true
    }

    /// Assign some margin for a trade with given margin value, denoted in BASE currency
    /// Return true if successful
    pub fn add_margin_to_position(&mut self, trade_margin: f64) -> bool {
        if trade_margin > self.available_balance {
            return false;
        }
        self.position_margin += trade_margin;
        self.available_balance -= trade_margin;

        true
    }

    /// Reduce the position margin by a given trade margin
    /// Return true if successful
    pub fn reduce_position_margin(&mut self, trade_margin: f64) -> bool {
        if trade_margin > self.position_margin {
            return false;
        }
        self.position_margin -= trade_margin;
        self.available_balance += trade_margin;

        true
    }

    /// Change the balance by a given delta e.g. from realizing profit or loss
    /// Return true if successful
    pub fn change_balance(&mut self, delta: f64) -> bool {
        let new_balance: f64 = self.wallet_balance + delta;
        if new_balance < 0.0 {
            return false;
        }
        self.wallet_balance = new_balance;
        self.margin_balance = new_balance;
        self.available_balance += delta;

        true
    }

    /// Return the wallet balance of account
    pub fn wallet_balance(&self) -> f64 {
        self.wallet_balance
    }

    /// Return the margin balance of account
    pub fn margin_balance(&self) -> f64 {
        self.margin_balance
    }

    /// Return the position margin of account
    pub fn position_margin(&self) -> f64 {
        self.position_margin
    }

    /// Return the used order margin of account
    pub fn order_margin(&self) -> f64 {
        self.order_margin
    }

    /// Return the available balance of account
    pub fn available_balance(&self) -> f64 {
        self.available_balance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margin_reserve_order_margin() {
        let mut margin = Margin::new(1.0);

        let success: bool = margin.reserve_order_margin(0.1);
        assert!(success);
        assert_eq!(margin.wallet_balance, 1.0);
        assert_eq!(margin.margin_balance, 1.0);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 0.1);
        assert_eq!(margin.available_balance, 0.9);
    }

    #[test]
    fn margin_free_order_margin() {
        let mut margin = Margin::new(1.0);

        let order_margin: f64 = 0.1;
        let success: bool = margin.reserve_order_margin(order_margin);
        assert!(success);

        let success: bool = margin.free_order_margin(order_margin);
        assert!(success);
        assert_eq!(margin.wallet_balance, 1.0);
        assert_eq!(margin.margin_balance, 1.0);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(margin.available_balance, 1.0);
    }

    #[test]
    fn margin_add_margin_to_position() {
        let mut margin = Margin::new(1.0);

        let success: bool = margin.add_margin_to_position(0.25);
        assert!(success);
        assert_eq!(margin.wallet_balance, 1.0);
        assert_eq!(margin.margin_balance, 1.0);
        assert_eq!(margin.position_margin, 0.25);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(margin.available_balance, 0.75);
    }

    #[test]
    #[ignore]
    fn margin_reduce_position_margin() {
        let mut margin = Margin::new(1.0);

        let success: bool = margin.add_margin_to_position(0.25);
        assert!(success);

        // this represents a profitable trade. profit = 0.05
        margin.reduce_position_margin(0.3);
        assert_eq!(margin.wallet_balance, 1.05);
        assert_eq!(margin.margin_balance, 1.05);
        assert_eq!(margin.position_margin, 0.25);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(margin.available_balance, 0.75);
    }

    #[test]
    fn margin_change_balance() {
        let mut margin = Margin::new(1.0);

        let success: bool = margin.change_balance(0.05);
        assert!(success);
        assert_eq!(margin.wallet_balance, 1.05);
        assert_eq!(margin.margin_balance, 1.05);
        assert_eq!(margin.position_margin, 0.0);
        assert_eq!(margin.order_margin, 0.0);
        assert_eq!(margin.available_balance, 1.05);
    }
}
