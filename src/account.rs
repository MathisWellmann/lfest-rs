use crate::acc_tracker::AccTracker;
use crate::{FuturesTypes, Margin, Order, Position, Side};
use hashbrown::HashMap;

#[derive(Debug, Clone)]
/// The users account
pub struct Account {
    futures_type: FuturesTypes,
    margin: Margin,
    position: Position,
    acc_tracker: AccTracker,
    // TODO: merge this field with order_margins into active_limit_orders of Type HashMap<u64, (Order, f64)>
    active_limit_orders: Vec<Order>,
    // maps the order id to reserved margin
    order_margins: HashMap<u64, f64>,
    executed_orders: Vec<Order>,
    // used for calculating hedged order size for order margin calculation
    open_limit_buy_size: f64,
    open_limit_sell_size: f64,
    // TODO: remove following two fields
    min_limit_buy_price: f64,
    max_limit_sell_price: f64,
}

impl Account {
    pub fn new(leverage: f64, starting_balance: f64, futures_type: FuturesTypes) -> Self {
        let position = Position::new_init(leverage);
        let margin = Margin::new_init(starting_balance);
        let acc_tracker = AccTracker::new(starting_balance);
        Self {
            futures_type,
            margin,
            position,
            acc_tracker,
            active_limit_orders: vec![],
            order_margins: HashMap::new(),
            executed_orders: vec![],
            open_limit_buy_size: 0.0,
            open_limit_sell_size: 0.0,
            min_limit_buy_price: 0.0,
            max_limit_sell_price: 0.0,
        }
    }

    /// Update the accounts state for the newest price data
    pub fn update(&mut self, price: f64, trade_timestamp: u64) {
        self.acc_tracker.log_timestamp(trade_timestamp);
        let upnl: f64 =
            self.futures_type
                .pnl(self.position.entry_price(), price, self.position.size());
        self.acc_tracker.log_upnl(upnl);

        self.position.update_state(price, self.futures_type);
    }

    /// Set a new position manually, be sure that you know what you are doing
    /// Returns true if successful
    #[inline(always)]
    pub fn set_position(&mut self, position: Position) {
        self.position = position;
    }

    /// Return a reference to position
    #[inline(always)]
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Set a new margin manually, be sure that you know what you are doing when using this method
    /// Returns true if successful
    #[inline(always)]
    pub fn set_margin(&mut self, margin: Margin) {
        self.margin = margin;
    }

    /// Return a reference to margin
    #[inline(always)]
    pub fn margin(&self) -> &Margin {
        &self.margin
    }

    /// Return recently executed orders
    /// and clear them afterwards
    pub(crate) fn executed_orders(&mut self) -> Vec<Order> {
        let exec_orders = self.executed_orders.clone();
        self.executed_orders.clear();

        exec_orders
    }

    /// Return the currently active limit orders
    #[inline(always)]
    pub fn active_limit_orders(&self) -> &Vec<Order> {
        &self.active_limit_orders
    }

    /// Return a reference to acc_tracker struct
    #[inline(always)]
    pub fn acc_tracker(&self) -> &AccTracker {
        &self.acc_tracker
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Option<Order> {
        for (i, o) in self.active_limit_orders.iter().enumerate() {
            if o.id() == order_id {
                let old_order = self.active_limit_orders.remove(i);
                match old_order.side() {
                    Side::Buy => self.open_limit_buy_size -= old_order.size(),
                    Side::Sell => self.open_limit_sell_size -= old_order.size(),
                }
                return Some(old_order);
            }
        }

        // re compute min and max prices for open orders
        self.min_limit_buy_price = 0.0;
        self.max_limit_sell_price = 0.0;
        for o in self.active_limit_orders.iter() {
            let limit_price = o.limit_price().unwrap();
            match o.side() {
                Side::Buy => {
                    if self.min_limit_buy_price == 0.0 {
                        self.min_limit_buy_price = limit_price;
                        continue;
                    }
                    if limit_price < self.min_limit_buy_price {
                        self.min_limit_buy_price = limit_price;
                    }
                }
                Side::Sell => {
                    if self.max_limit_sell_price == 0.0 {
                        self.max_limit_sell_price = limit_price;
                        continue;
                    }
                    if limit_price > self.max_limit_sell_price {
                        self.max_limit_sell_price = limit_price;
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
        self.min_limit_buy_price = 0.0;
        self.max_limit_sell_price = 0.0;
        self.active_limit_orders.clear();
    }

    /// Append a new limit order as active order
    pub(crate) fn append_limit_order(&mut self, order: Order, order_margin: f64) {
        debug!(
            "append_limit_order: order: {:?}, order_margin: {}",
            order, order_margin
        );

        let limit_price = order.limit_price().unwrap();
        match order.side() {
            Side::Buy => {
                self.open_limit_buy_size += order.size();
                if self.min_limit_buy_price == 0.0 {
                    self.min_limit_buy_price = limit_price;
                }
                if limit_price < self.min_limit_buy_price {
                    self.min_limit_buy_price = limit_price;
                }
            }
            Side::Sell => {
                self.open_limit_sell_size += order.size();
                if self.max_limit_sell_price == 0.0 {
                    self.max_limit_sell_price = limit_price;
                }
                if limit_price > self.max_limit_sell_price {
                    self.max_limit_sell_price = limit_price;
                }
            }
        }
        // assigning order margin and closing out position are two different things

        self.order_margins.insert(order.id(), order_margin);
        let new_om = self.margin.order_margin() + order_margin;
        self.margin.set_order_margin(new_om);

        self.acc_tracker.log_limit_order_submission();
        self.active_limit_orders.push(order);
    }

    /// Remove the assigned order margin for a given order
    pub(crate) fn free_order_margin(&mut self, order_id: u64) {
        debug!("free_order_margin: {}", order_id);

        let om: f64 = *self.order_margins.get(&order_id).unwrap();
        let new_om = self.margin.order_margin() - om;
        self.margin.set_order_margin(new_om);
    }

    /// Finalize an executed limit order
    pub(crate) fn finalize_limit_order(&mut self, order_idx: usize) {
        let mut exec_order = self.active_limit_orders.remove(order_idx);

        exec_order.mark_executed();

        // free order margin
        match exec_order.side() {
            Side::Buy => self.open_limit_buy_size -= exec_order.size(),
            Side::Sell => self.open_limit_sell_size -= exec_order.size(),
        }
        // re-calculate min and max price
        self.min_limit_buy_price = if self.active_limit_orders.is_empty() {
            0.0
        } else {
            self.active_limit_orders
                .iter()
                .filter(|o| o.side() == Side::Buy)
                .map(|o| o.limit_price().unwrap())
                .sum()
        };
        self.max_limit_sell_price = if self.active_limit_orders.is_empty() {
            0.0
        } else {
            self.active_limit_orders
                .iter()
                .filter(|o| o.side() == Side::Sell)
                .map(|o| o.limit_price().unwrap())
                .sum()
        };

        self.acc_tracker.log_limit_order_fill();
        self.executed_orders.push(exec_order);
    }

    /// Reduce the account equity by a fee amount
    pub(crate) fn deduce_fees(&mut self, fee: f64) {
        debug!("account: deduce_fees: deducing {} in fees", fee);

        self.acc_tracker.log_fee(fee);
        self.margin.change_balance(-fee);
    }

    /// Changes the position by a given delta while changing margin accordingly
    pub(crate) fn change_position(&mut self, side: Side, size: f64, exec_price: f64) {
        debug!(
            "account: change_position(side: {:?}, size: {}, exec_price: {})",
            side, size, exec_price
        );
        let pos_size_delta: f64 = match side {
            Side::Buy => size,
            Side::Sell => -size,
        };
        let rpnl = match side {
            Side::Buy => {
                if self.position.size() < 0.0 {
                    // pnl needs to be realized
                    if size > self.position.size().abs() {
                        self.futures_type.pnl(
                            self.position.entry_price(),
                            exec_price,
                            self.position.size(),
                        )
                    } else {
                        self.futures_type
                            .pnl(self.position.entry_price(), exec_price, -size)
                    }
                } else {
                    0.0
                }
            }
            Side::Sell => {
                if self.position.size() > 0.0 {
                    // pnl needs to be realized
                    if size > self.position.size() {
                        self.futures_type.pnl(
                            self.position.entry_price(),
                            exec_price,
                            self.position.size(),
                        )
                    } else {
                        self.futures_type
                            .pnl(self.position.entry_price(), exec_price, size)
                    }
                } else {
                    0.0
                }
            }
        };
        if rpnl != 0.0 {
            // first free up existing position margin if any
            let mut new_pos_margin: f64 =
                (self.position().size() + pos_size_delta).abs() / self.position().leverage();
            match self.futures_type {
                FuturesTypes::Linear => new_pos_margin *= self.position.entry_price(),
                FuturesTypes::Inverse => new_pos_margin /= self.position.entry_price(),
            };
            self.margin.set_position_margin(new_pos_margin);

            self.margin.change_balance(rpnl);
            self.acc_tracker.log_rpnl(rpnl);
        }

        // change position
        self.position
            .change_size(pos_size_delta, exec_price, self.futures_type);

        // set position margin
        let mut pos_margin: f64 = self.position.size().abs() / self.position.leverage();
        match self.futures_type {
            FuturesTypes::Linear => pos_margin *= self.position.entry_price(),
            FuturesTypes::Inverse => pos_margin /= self.position.entry_price(),
        };
        self.margin.set_position_margin(pos_margin);

        // log change
        self.acc_tracker.log_trade(side, size);
    }

    #[inline(always)]
    pub(crate) fn open_limit_buy_size(&self) -> f64 {
        self.open_limit_buy_size
    }

    #[inline(always)]
    pub(crate) fn open_limit_sell_size(&self) -> f64 {
        self.open_limit_sell_size
    }

    #[inline(always)]
    pub(crate) fn min_limit_buy_price(&self) -> f64 {
        self.min_limit_buy_price
    }

    #[inline(always)]
    pub(crate) fn max_limit_sell_price(&self) -> f64 {
        self.max_limit_sell_price
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FuturesTypes, Validator};

    #[test]
    fn account_append_limit_order() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut acc = Account::new(1.0, 1.0, futures_type);
        let mut validator = Validator::new(0.0, 0.0, futures_type);
        validator.update(100.0, 101.0);

        let o = Order::limit(Side::Buy, 100.0, 25.0).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, 25.0);
        assert_eq!(acc.open_limit_sell_size, 0.0);
        assert_eq!(acc.min_limit_buy_price, 100.0);
        assert_eq!(acc.max_limit_sell_price, 0.0);
        assert_eq!(acc.margin().order_margin(), 0.25);
        assert_eq!(acc.margin().available_balance(), 0.75);

        let o = Order::limit(Side::Sell, 100.0, 25.0).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, 25.0);
        assert_eq!(acc.open_limit_sell_size, 25.0);
        assert_eq!(acc.min_limit_buy_price, 100.0);
        assert_eq!(acc.max_limit_sell_price, 100.0);
        assert_eq!(acc.margin().order_margin(), 0.25);
        assert_eq!(acc.margin().available_balance(), 0.75);

        let o = Order::limit(Side::Buy, 90.0, 25.0).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, 50.0);
        assert_eq!(acc.open_limit_sell_size, 25.0);
        assert_eq!(acc.min_limit_buy_price, 90.0);
        assert_eq!(acc.max_limit_sell_price, 100.0);
        // TODO: what is the proper test result here
        // assert_eq!(account.margin().order_margin(), 0.5278);
        // assert_eq!(account.margin().available_balance(), 0.75);

        let o = Order::limit(Side::Sell, 110.0, 25.0).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, 50.0);
        assert_eq!(acc.open_limit_sell_size, 50.0);
        assert_eq!(acc.min_limit_buy_price, 90.0);
        assert_eq!(acc.max_limit_sell_price, 110.0);
        // assert_eq!(account.margin().order_margin(), 0.5278);
        // assert_eq!(account.margin().available_balance(), 0.75);
    }

    #[test]
    fn account_cancel_order() {
        let futures_type = FuturesTypes::Inverse;
        let mut account = Account::new(1.0, 1.0, futures_type);
        let mut validator = Validator::new(0.0, 0.0, futures_type);
        validator.update(900.0, 901.0);

        let o = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        account.append_limit_order(o, order_margin);
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
        let futures_type = FuturesTypes::Inverse;
        let mut account = Account::new(1.0, 1.0, futures_type);
        let mut validator = Validator::new(0.0, 0.0, futures_type);
        validator.update(900.0, 901.0);

        let o = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        account.append_limit_order(o, order_margin);
        assert_eq!(account.active_limit_orders().len(), 1);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);
        assert_eq!(account.margin().order_margin(), 0.5);
        assert_eq!(account.margin().available_balance(), 0.5);

        account.cancel_all_orders();
        assert_eq!(account.active_limit_orders().len(), 0);
        assert_eq!(account.margin().wallet_balance(), 1.0);
        assert_eq!(account.margin().position_margin(), 0.0);
        assert_eq!(account.margin().order_margin(), 0.0);
        assert_eq!(account.margin().available_balance(), 1.0);
    }

    #[test]
    fn account_change_position_inverse_future() {
        let mut acc = Account::new(1.0, 1.0, FuturesTypes::Inverse);

        acc.change_position(Side::Buy, 100.0, 200.0);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.5);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.5);
        assert_eq!(acc.position().size(), 100.0);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);

        acc.change_position(Side::Sell, 100.0, 200.0);
        assert_eq!(acc.position().size(), 0.0);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.0);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 1.0);

        acc.change_position(Side::Sell, 100.0, 200.0);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.5);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.5);
        assert_eq!(acc.position().size(), -100.0);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);

        acc.change_position(Side::Buy, 150.0, 200.0);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.25);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.75);
        assert_eq!(acc.position().size(), 50.0);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);

        acc.change_position(Side::Sell, 25.0, 200.0);
        assert_eq!(acc.margin().wallet_balance(), 1.0);
        assert_eq!(acc.margin().position_margin(), 0.125);
        assert_eq!(acc.margin().order_margin(), 0.0);
        assert_eq!(acc.margin().available_balance(), 0.875);
        assert_eq!(acc.position().size(), 25.0);
        assert_eq!(acc.position().entry_price(), 200.0);
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), 0.0);
    }

    #[test]
    fn account_change_position_linear_futures() {
        // TODO:
    }
}
