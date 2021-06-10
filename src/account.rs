use crate::acc_tracker::AccTracker;
use crate::{max, min, FuturesType, Margin, Order, OrderType, Position, Side};

#[derive(Debug, Clone)]
/// The users account
pub struct Account {
    margin: Margin,
    position: Position,
    acc_tracker: AccTracker,
    active_limit_orders: Vec<Order>,
    active_stop_orders: Vec<Order>,
    executed_orders: Vec<Order>,
    // used for calculating hedged order size for order margin calculation
    pub(crate) open_limit_buy_size: f64,
    pub(crate) open_limit_sell_size: f64,
    pub(crate) open_stop_buy_size: f64,
    pub(crate) open_stop_sell_size: f64,
    pub(crate) min_limit_buy_price: f64,
    pub(crate) max_limit_sell_price: f64,
    pub(crate) max_stop_buy_price: f64,
    pub(crate) min_stop_sell_price: f64,
}

impl Account {
    pub fn new(leverage: f64, starting_balance: f64) -> Self {
        let position = Position::new(leverage);
        let margin = Margin::new_init(starting_balance);
        let acc_tracker = AccTracker::new(starting_balance);
        Self {
            margin,
            position,
            acc_tracker,
            active_limit_orders: vec![],
            active_stop_orders: vec![],
            executed_orders: vec![],
            open_limit_buy_size: 0.0,
            open_limit_sell_size: 0.0,
            open_stop_buy_size: 0.0,
            open_stop_sell_size: 0.0,
            min_limit_buy_price: 0.0,
            max_limit_sell_price: 0.0,
            max_stop_buy_price: 0.0,
            min_stop_sell_price: 0.0,
        }
    }

    /// Update the accounts state for the newest price data
    pub fn update(&mut self, price: f64, trade_timestamp: u64) {
        self.acc_tracker.log_timestamp(trade_timestamp);
        let upnl = self.position.size() * ((1.0 / self.position.entry_price()) - (1.0 / price));
        self.acc_tracker.log_upnl(upnl);

        self.position.update_state(price);
    }

    /// Set a new position manually, be sure that you know what you are doing
    /// Returns true if successful
    pub fn set_position(&mut self, position: Position) {
        self.position = position;
    }

    /// Return a reference to position
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Set a new margin manually, be sure that you know what you are doing when using this method
    /// Returns true if successful
    pub fn set_margin(&mut self, margin: Margin) {
        self.margin = margin;
    }

    /// Return a reference to margin
    pub fn margin(&self) -> &Margin {
        &self.margin
    }

    /// Return recently executed orders
    /// and clear them afterwards
    pub fn executed_orders(&mut self) -> Vec<Order> {
        let exec_orders = self.executed_orders.clone();
        self.executed_orders.clear();

        exec_orders
    }

    /// Return the currently active limit orders
    pub fn active_limit_orders(&self) -> &Vec<Order> {
        &self.active_limit_orders
    }

    /// Return the currently active stop orders
    pub fn active_stop_orders(&self) -> &Vec<Order> {
        &self.active_stop_orders
    }

    /// Return a reference to acc_tracker struct
    pub fn acc_tracker(&self) -> &AccTracker {
        &self.acc_tracker
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<Order> {
        for (i, o) in self.active_limit_orders.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.active_limit_orders.remove(i);
                match old_order.side {
                    Side::Buy => self.open_limit_buy_size -= old_order.size,
                    Side::Sell => self.open_limit_sell_size -= old_order.size,
                }
                return Some(old_order);
            }
        }
        for (i, o) in self.active_stop_orders.iter().enumerate() {
            if o.id == order_id {
                let old_order = self.active_stop_orders.remove(i);
                match old_order.side {
                    Side::Buy => self.open_stop_buy_size -= old_order.size,
                    Side::Sell => self.open_stop_sell_size -= old_order.size,
                }
                return Some(old_order);
            }
        }
        // re compute min and max prices for open orders
        self.min_limit_buy_price = 0.0;
        self.max_limit_sell_price = 0.0;
        for o in self.active_limit_orders.iter() {
            match o.side {
                Side::Buy => {
                    if self.min_limit_buy_price == 0.0 {
                        self.min_limit_buy_price = o.limit_price;
                        continue;
                    }
                    if o.limit_price < self.min_limit_buy_price {
                        self.min_limit_buy_price = o.limit_price;
                    }
                }
                Side::Sell => {
                    if self.max_limit_sell_price == 0.0 {
                        self.max_limit_sell_price = o.limit_price;
                        continue;
                    }
                    if o.limit_price > self.max_limit_sell_price {
                        self.max_limit_sell_price = o.limit_price;
                    }
                }
            }
        }

        self.max_stop_buy_price = 0.0;
        self.min_stop_sell_price = 0.0;
        for o in self.active_stop_orders.iter() {
            match o.side {
                Side::Buy => {
                    if self.max_stop_buy_price == 0.0 {
                        self.max_stop_buy_price = o.trigger_price;
                        continue;
                    }
                    if o.trigger_price > self.max_stop_buy_price {
                        self.max_stop_buy_price = o.trigger_price;
                    }
                }
                Side::Sell => {
                    if self.min_stop_sell_price == 0.0 {
                        self.min_stop_sell_price = o.trigger_price;
                        continue;
                    }
                    if o.trigger_price < self.min_stop_sell_price {
                        self.min_stop_sell_price = o.trigger_price;
                    }
                }
            }
        }
        None
    }

    /// Cancel all active orders
    pub fn cancel_all_orders(&mut self) {
        self.margin.set_order_margin(0.0);
        self.open_limit_buy_size = 0.0;
        self.open_limit_sell_size = 0.0;
        self.open_stop_buy_size = 0.0;
        self.open_stop_sell_size = 0.0;
        self.min_limit_buy_price = 0.0;
        self.max_limit_sell_price = 0.0;
        self.max_stop_buy_price = 0.0;
        self.min_stop_sell_price = 0.0;
        self.active_limit_orders.clear();
        self.active_stop_orders.clear();
    }

    /// append order to active orders and update internal state accordingly
    pub(crate) fn append_order(&mut self, order: Order) {
        match order.order_type {
            OrderType::Limit => self.append_limit_order(order),
            OrderType::StopMarket => self.append_stop_order(order),
            OrderType::Market => {}
        }
    }

    /// Append a new limit order as active order
    fn append_limit_order(&mut self, order: Order) {
        match order.side {
            Side::Buy => {
                self.open_limit_buy_size += order.size;
                if self.min_limit_buy_price == 0.0 {
                    self.min_limit_buy_price = order.limit_price;
                }
                if order.limit_price < self.min_limit_buy_price {
                    self.min_limit_buy_price = order.limit_price;
                }
            }
            Side::Sell => {
                self.open_limit_sell_size += order.size;
                if self.max_limit_sell_price == 0.0 {
                    self.max_limit_sell_price = order.limit_price;
                }
                if order.limit_price > self.max_limit_sell_price {
                    self.max_limit_sell_price = order.limit_price;
                }
            }
        }
        // set order margin
        self.margin.set_order_margin(self.order_margin());

        self.acc_tracker.log_limit_order_submission();
        self.active_limit_orders.push(order.clone());
    }

    /// Append a new stop order as active order
    fn append_stop_order(&mut self, order: Order) {
        match order.side {
            Side::Buy => {
                self.open_stop_buy_size += order.size;
                if self.max_stop_buy_price == 0.0 {
                    self.max_stop_buy_price = order.trigger_price;
                }
                if order.trigger_price > self.max_stop_buy_price {
                    self.max_stop_buy_price = order.trigger_price;
                }
            }
            Side::Sell => {
                self.open_stop_sell_size += order.size;
                if self.min_stop_sell_price == 0.0 {
                    self.min_stop_sell_price = order.trigger_price;
                }
                if order.trigger_price < self.min_stop_sell_price {
                    self.min_stop_sell_price = order.trigger_price;
                }
            }
        }
        // set order margin
        self.margin.set_order_margin(self.order_margin());

        self.active_stop_orders.push(order.clone());
    }

    /// Finalize an executed limit order
    pub(crate) fn finalize_limit_order(&mut self, order_idx: usize) {
        let mut exec_order = self.active_limit_orders.remove(order_idx);

        exec_order.mark_executed();

        // free order margin
        match exec_order.side {
            Side::Buy => self.open_limit_buy_size -= exec_order.size,
            Side::Sell => self.open_limit_sell_size -= exec_order.size,
        }
        // re-calculate min and max price
        self.min_limit_buy_price = if self.active_limit_orders.len() == 0 {
            0.0
        } else {
            self.active_limit_orders
                .iter()
                .filter(|o| o.side == Side::Buy)
                .map(|o| o.limit_price)
                .sum()
        };
        self.max_limit_sell_price = if self.active_limit_orders.len() == 0 {
            0.0
        } else {
            self.active_limit_orders
                .iter()
                .filter(|o| o.side == Side::Sell)
                .map(|o| o.limit_price)
                .sum()
        };

        self.margin.set_order_margin(self.order_margin());

        self.executed_orders.push(exec_order);
    }

    /// Finalize an executed stop order
    pub(crate) fn finalize_stop_order(&mut self, order_idx: usize) {
        let mut exec_order = self.active_stop_orders.remove(order_idx);

        exec_order.mark_executed();

        // free order margin
        match exec_order.side {
            Side::Buy => self.open_stop_buy_size -= exec_order.size,
            Side::Sell => self.open_stop_sell_size -= exec_order.size,
        }
        // re-calculate min and max price
        self.min_stop_sell_price = if self.active_stop_orders.len() == 0 {
            0.0
        } else {
            self.active_stop_orders
                .iter()
                .filter(|o| o.side == Side::Sell)
                .map(|o| o.trigger_price)
                .sum()
        };
        self.max_stop_buy_price = if self.active_stop_orders.len() == 0 {
            0.0
        } else {
            self.active_stop_orders
                .iter()
                .filter(|o| o.side == Side::Buy)
                .map(|o| o.trigger_price)
                .sum()
        };

        self.margin.set_order_margin(self.order_margin());

        self.executed_orders.push(exec_order);
    }

    /// Reduce the account equity by a fee amount
    pub(crate) fn deduce_fees(&mut self, fee: f64) {
        self.acc_tracker.log_fee(fee);
        self.margin.change_balance(-fee);
    }

    /// Changes the position by a given delta while changing margin accordingly
    pub(crate) fn change_position(
        &mut self,
        side: Side,
        size: f64,
        price: f64,
        futures_type: FuturesType,
    ) {
        let pos_size_delta: f64 = match side {
            Side::Buy => size,
            Side::Sell => -size,
        };
        let rpnl = match side {
            Side::Buy => {
                if self.position.size() < 0.0 {
                    // pnl needs to be realized
                    if size > self.position.size().abs() {
                        futures_type.pnl(self.position.entry_price(), price, self.position.size())
                    } else {
                        futures_type.pnl(self.position.entry_price(), price, -size)
                    }
                } else {
                    0.0
                }
            }
            Side::Sell => {
                if self.position.size() > 0.0 {
                    // pnl needs to be realized
                    if size > self.position.size() {
                        futures_type.pnl(self.position.entry_price(), price, self.position.size())
                    } else {
                        futures_type.pnl(self.position.entry_price(), price, size)
                    }
                } else {
                    0.0
                }
            }
        };
        if rpnl != 0.0 {
            self.margin.change_balance(rpnl);
            self.acc_tracker.log_rpnl(rpnl);
        }

        // change position
        self.position.change_size(pos_size_delta, price);
        self.margin.set_position_margin(
            self.position.size().abs() / self.position.entry_price() / self.position.leverage(),
        );

        // log change
        self.acc_tracker.log_trade(side, size);
    }

    /// Calculate the order margin
    /// TODO: mark it work in all cases
    fn order_margin(&self) -> f64 {
        let ps: f64 = self.position.size();
        let open_sizes: [f64; 4] = [
            self.open_limit_buy_size,
            self.open_limit_sell_size,
            self.open_stop_buy_size,
            self.open_stop_sell_size,
        ];
        let mut max_idx: usize = 0;
        let mut m: f64 = self.open_limit_buy_size;

        for (i, s) in open_sizes.iter().enumerate() {
            if *s > m {
                m = *s;
                max_idx = i;
            }
        }

        // direction of dominating open order side
        let (d, p) = match max_idx {
            0 => (1.0, self.min_limit_buy_price),
            1 => (-1.0, self.max_limit_sell_price),
            2 => (1.0, self.max_stop_buy_price),
            3 => (-1.0, self.min_stop_sell_price),
            _ => panic!("any other value should not be possible"),
        };
        if p == 0.0 {
            return 0.0;
        }

        max(0.0, min(m, m + d * ps)) / p / self.position.leverage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::round;
    use crate::FuturesType;

    #[test]
    fn account_append_limit_order() {
        let mut account = Account::new(1.0, 1.0);

        account.append_limit_order(Order::limit(Side::Buy, 100.0, 25.0).unwrap());
        assert_eq!(account.open_limit_buy_size, 25.0);
        assert_eq!(account.open_limit_sell_size, 0.0);
        assert_eq!(account.min_limit_buy_price, 100.0);
        assert_eq!(account.max_limit_sell_price, 0.0);
        assert_eq!(account.margin().order_margin(), 0.25);
        assert_eq!(account.margin().available_balance(), 0.75);

        account.append_limit_order(Order::limit(Side::Sell, 100.0, 25.0).unwrap());
        assert_eq!(account.open_limit_buy_size, 25.0);
        assert_eq!(account.open_limit_sell_size, 25.0);
        assert_eq!(account.min_limit_buy_price, 100.0);
        assert_eq!(account.max_limit_sell_price, 100.0);
        assert_eq!(account.margin().order_margin(), 0.25);
        assert_eq!(account.margin().available_balance(), 0.75);

        account.append_limit_order(Order::limit(Side::Buy, 90.0, 25.0).unwrap());
        assert_eq!(account.open_limit_buy_size, 50.0);
        assert_eq!(account.open_limit_sell_size, 25.0);
        assert_eq!(account.min_limit_buy_price, 90.0);
        assert_eq!(account.max_limit_sell_price, 100.0);
        // TODO: what is the proper test result here
        // assert_eq!(account.margin().order_margin(), 0.5278);
        // assert_eq!(account.margin().available_balance(), 0.75);

        account.append_limit_order(Order::limit(Side::Sell, 110.0, 25.0).unwrap());
        assert_eq!(account.open_limit_buy_size, 50.0);
        assert_eq!(account.open_limit_sell_size, 50.0);
        assert_eq!(account.min_limit_buy_price, 90.0);
        assert_eq!(account.max_limit_sell_price, 110.0);
        // assert_eq!(account.margin().order_margin(), 0.5278);
        // assert_eq!(account.margin().available_balance(), 0.75);
    }

    #[test]
    fn account_append_stop_order() {
        let mut account = Account::new(1.0, 1.0);

        account.append_stop_order(Order::stop_market(Side::Buy, 100.0, 25.0).unwrap());
        assert_eq!(account.open_stop_buy_size, 25.0);
        assert_eq!(account.open_stop_sell_size, 0.0);
        assert_eq!(account.max_stop_buy_price, 100.0);
        assert_eq!(account.min_stop_sell_price, 0.0);

        account.append_stop_order(Order::stop_market(Side::Sell, 100.0, 25.0).unwrap());
        assert_eq!(account.open_stop_buy_size, 25.0);
        assert_eq!(account.open_stop_sell_size, 25.0);
        assert_eq!(account.max_stop_buy_price, 100.0);
        assert_eq!(account.min_stop_sell_price, 100.0);

        account.append_stop_order(Order::stop_market(Side::Buy, 110.0, 25.0).unwrap());
        assert_eq!(account.open_stop_buy_size, 50.0);
        assert_eq!(account.open_stop_sell_size, 25.0);
        assert_eq!(account.max_stop_buy_price, 110.0);
        assert_eq!(account.min_stop_sell_price, 100.0);

        account.append_stop_order(Order::stop_market(Side::Sell, 90.0, 25.0).unwrap());
        assert_eq!(account.open_stop_buy_size, 50.0);
        assert_eq!(account.open_stop_sell_size, 50.0);
        assert_eq!(account.max_stop_buy_price, 110.0);
        assert_eq!(account.min_stop_sell_price, 90.0);

        // TODO: test order_margin
    }

    #[test]
    fn account_cancel_order() {
        let mut account = Account::new(1.0, 1.0);

        let o = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        account.append_order(o);
        assert_eq!(account.active_limit_orders().len(), 1);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);

        account.cancel_order(0);
        assert_eq!(account.active_limit_orders().len(), 0);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);
    }

    #[test]
    fn account_cancel_all_orders() {
        let mut account = Account::new(1.0, 1.0);

        let o = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        account.append_order(o);
        assert_eq!(account.active_limit_orders().len(), 1);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);
        assert_eq!(account.margin().order_margin(), 0.5);
        assert_eq!(account.margin().available_balance(), 0.5);

        let o = Order::stop_market(Side::Buy, 1100.0, 450.0).unwrap();
        account.append_order(o);
        assert_eq!(account.active_limit_orders().len(), 1);
        assert_eq!(account.active_stop_orders().len(), 1);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);
        assert_eq!(round(account.margin().order_margin(), 1), 0.5);
        assert_eq!(account.margin().available_balance(), 0.5);

        account.cancel_all_orders();
        assert_eq!(account.active_limit_orders().len(), 0);
        assert_eq!(account.active_stop_orders().len(), 0);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);
        assert_eq!(account.margin().order_margin(), 0.0);
        assert_eq!(account.order_margin(), 0.0);
        assert_eq!(account.margin().available_balance(), 1.0);
    }

    #[test]
    fn account_order_margin() {
        let mut account = Account::new(1.0, 1.0);

        account.append_order(Order::limit(Side::Buy, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 0.5);

        account.append_order(Order::limit(Side::Sell, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 0.5);

        account.append_order(Order::stop_market(Side::Sell, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 0.5);

        account.append_order(Order::stop_market(Side::Buy, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 0.5);

        account.append_order(Order::limit(Side::Buy, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 1.0);

        account.append_order(Order::limit(Side::Sell, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 1.0);

        account.append_order(Order::stop_market(Side::Sell, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 1.0);

        account.append_order(Order::stop_market(Side::Buy, 100.0, 50.0).unwrap());
        assert_eq!(account.order_margin(), 1.0);
    }

    #[test]
    fn account_change_position_size_inverse_future() {
        let mut acc = Account::new(1.0, 1.0);

        let futures_type = FuturesType::Inverse;
        acc.change_position(Side::Buy, 100.0, 200.0, futures_type);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.5);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.5);
        assert_eq!(acc.position().size(), 100.0);
        assert_eq!(acc.position().value(), 0.5);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);

        acc.change_position(Side::Sell, 100.0, 200.0, futures_type);
        assert_eq!(acc.position().size(), 0.0);
        assert_eq!(acc.position().value(), 0.0);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.0);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 1.0);

        acc.change_position(Side::Sell, 100.0, 200.0, futures_type);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.5);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.5);
        assert_eq!(acc.position().size(), -100.0);
        assert_eq!(acc.position().value(), 0.5);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);

        acc.change_position(Side::Buy, 150.0, 200.0, futures_type);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.25);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.75);
        assert_eq!(acc.position().size(), 50.0);
        assert_eq!(acc.position().value(), 0.25);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);

        acc.change_position(Side::Sell, 25.0, 200.0, futures_type);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.125);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.875);
        assert_eq!(acc.position().size(), 25.0);
        assert_eq!(acc.position().value(), 0.125);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);
    }
}
