use fpdec::Decimal;
use hashbrown::HashMap;

use crate::{
    account_tracker::AccountTracker,
    errors::{Error, Result},
    margin::Margin,
    position::Position,
    quote,
    types::{Currency, Fee, Leverage, MarginCurrency, Order, QuoteCurrency},
};

#[derive(Debug, Clone)]
/// The users account
/// Generic over:
/// A: AccountTracker,
/// S: The `Currency` representing the order quantity
/// B: Balance type
pub struct Account<A, S>
where
    S: Currency + Default,
{
    account_tracker: A,
    margin: Margin<S::PairedCurrency>,
    position: Position<S>,
    active_limit_orders: HashMap<u64, Order<S>>,
    lookup_id_from_user_order_id: HashMap<u64, u64>,
    executed_orders: Vec<Order<S>>,
}

impl<A, S> Account<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency + Default,
    S::PairedCurrency: MarginCurrency,
{
    pub(crate) fn new(
        account_tracker: A,
        leverage: Leverage,
        starting_balance: S::PairedCurrency,
    ) -> Self {
        let position = Position::default();
        let margin = Margin::new_init(starting_balance);

        Self {
            account_tracker,
            margin,
            position,
            active_limit_orders: HashMap::new(),
            lookup_id_from_user_order_id: HashMap::new(),
            executed_orders: vec![],
        }
    }

    /// Update the accounts state for the newest price data
    pub(crate) fn update(&mut self, bid: QuoteCurrency, ask: QuoteCurrency, trade_timestamp: u64) {
        let upnl = self.position().unrealized_pnl(bid, ask);
        let mid_price = (bid + ask) / quote!(2);
        self.account_tracker
            .update(trade_timestamp, mid_price, upnl);
    }

    /// The number of currently active limit orders
    #[inline(always)]
    pub(crate) fn num_active_limit_orders(&self) -> usize {
        self.active_limit_orders.len()
    }

    /// Set a new position manually, be sure that you know what you are doing
    #[inline(always)]
    pub fn set_position(&mut self, position: Position<S>) {
        self.position = position;
    }

    /// Return a reference to position
    #[inline(always)]
    pub fn position(&self) -> &Position<S> {
        &self.position
    }

    /// Set a new margin manually, be sure that you know what you are doing when
    /// using this method Returns true if successful
    #[inline(always)]
    pub fn set_margin(&mut self, margin: Margin<S::PairedCurrency>) {
        self.margin = margin;
    }

    /// Return a reference to margin
    #[inline(always)]
    pub fn margin(&self) -> &Margin<S::PairedCurrency> {
        &self.margin
    }

    /// Return recently executed orders
    /// and clear them afterwards
    pub(crate) fn executed_orders(&mut self) -> Vec<Order<S>> {
        let exec_orders = self.executed_orders.clone();
        self.executed_orders.clear();

        exec_orders
    }

    /// Return the currently active limit orders
    #[inline(always)]
    pub fn active_limit_orders(&self) -> &HashMap<u64, Order<S>> {
        &self.active_limit_orders
    }

    /// Return a reference to acc_tracker struct
    #[inline(always)]
    pub fn account_tracker(&self) -> &A {
        &self.account_tracker
    }

    /// Return a mutable reference to acc_tracker struct
    #[inline(always)]
    pub fn account_tracker_mut(&mut self) -> &mut A {
        &mut self.account_tracker
    }

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Result<Order<S>> {
        debug!("cancel_order: {}", order_id);
        let removed_order = match self.active_limit_orders.remove(&order_id) {
            None => return Err(Error::OrderIdNotFound),
            Some(o) => o,
        };

        self.account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Cancel an active order based on the user_order_id of an Order
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    pub fn cancel_order_by_user_id(&mut self, user_order_id: u64) -> Result<Order<S>> {
        debug!("cancel_order_by_user_id: user_order_id: {}", user_order_id);
        let id: u64 = match self.lookup_id_from_user_order_id.remove(&user_order_id) {
            None => return Err(Error::UserOrderIdNotFound),
            Some(id) => id,
        };
        self.cancel_order(id)
    }

    /// Cancel all active orders
    pub fn cancel_all_orders(&mut self) {
        debug!("cancel_all_orders");

        self.margin.clear_order_margin();
        self.active_limit_orders.clear();
    }

    /// Append a new limit order as active order
    #[deprecated]
    pub(crate) fn append_limit_order(&mut self, order: Order<S>, order_margin: S::PairedCurrency) {
        debug!(
            "append_limit_order: order: {:?}, order_margin: {}",
            order, order_margin
        );

        self.margin.lock_as_order_collateral(order_margin);

        self.account_tracker.log_limit_order_submission();
        let order_id = order.id();
        let user_order_id = *order.user_order_id();
        match self.active_limit_orders.insert(order_id, order) {
            None => {}
            Some(_) => warn!(
                "there already was an order with this id in active_limit_orders. \
            This should not happen as order id should be incrementing"
            ),
        };
        match user_order_id {
            None => {}
            Some(user_order_id) => {
                self.lookup_id_from_user_order_id
                    .insert(user_order_id, order_id);
            }
        };
    }

    /// Finalize an executed limit order
    #[deprecated]
    pub(crate) fn finalize_limit_order(&mut self, mut exec_order: Order<S>, fee_maker: Fee) {
        exec_order.mark_executed();

        self.account_tracker.log_limit_order_fill();
        self.executed_orders.push(exec_order);

        todo!()
        // TODO:
        // let new_om = order_margin(
        //     self.active_limit_orders.values().cloned(),
        //     &self.position,
        //     fee_maker,
        // );
        // self.margin.set_order_margin(new_om);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn account_append_limit_order() {
        if let Err(_) = pretty_env_logger::try_init() {}

        todo!()
        // let mut acc = Account::new(NoAccountTracker::default(), leverage!(1.0), base!(1.0));
        // let mut validator = Validator::new(fee!(0.0), fee!(0.0), 100);
        // validator.update(quote!(100.0), quote!(101.0));

        // let o = Order::limit(Side::Buy, quote!(100.0), quote!(25.0)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size, quote!(25.0));
        // assert_eq!(acc.open_limit_sell_size, quote!(0.0));
        // assert_eq!(acc.min_limit_buy_price, quote!(100.0));
        // assert_eq!(acc.max_limit_sell_price, quote!(0.0));
        // assert_eq!(acc.margin().order_margin(), base!(0.25));
        // assert_eq!(acc.margin().available_balance(), base!(0.75));

        // let o = Order::limit(Side::Sell, quote!(100.0), quote!(25.0)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size, quote!(25.0));
        // assert_eq!(acc.open_limit_sell_size, quote!(25.0));
        // assert_eq!(acc.min_limit_buy_price, quote!(100.0));
        // assert_eq!(acc.max_limit_sell_price, quote!(100.0));
        // assert_eq!(acc.margin().order_margin(), base!(0.25));
        // assert_eq!(acc.margin().available_balance(), base!(0.75));

        // let o = Order::limit(Side::Buy, quote!(90.0), quote!(25.0)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size, quote!(50.0));
        // assert_eq!(acc.open_limit_sell_size, quote!(25.0));
        // assert_eq!(acc.min_limit_buy_price, quote!(90.0));
        // assert_eq!(acc.max_limit_sell_price, quote!(100.0));
        // // TODO: what is the proper test result here
        // // assert_eq!(account.margin().order_margin(), 0.5278);
        // // assert_eq!(account.margin().available_balance(), 0.75);

        // let o = Order::limit(Side::Sell, quote!(110.0), quote!(25.0)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size, quote!(50.0));
        // assert_eq!(acc.open_limit_sell_size, quote!(50.0));
        // assert_eq!(acc.min_limit_buy_price, quote!(90.0));
        // assert_eq!(acc.max_limit_sell_price, quote!(110.0));
        // // assert_eq!(account.margin().order_margin(), 0.5278);
        // // assert_eq!(account.margin().available_balance(), 0.75);
    }

    #[test]
    fn account_cancel_order() {
        todo!()
        // let mut account = Account::new(NoAccountTracker::default(), leverage!(1.0), base!(1.0));
        // let mut validator = Validator::new(fee!(0.0), fee!(0.0), 100);
        // validator.update(quote!(900.0), quote!(901.0));

        // let o = Order::limit(Side::Buy, quote!(900.0), quote!(450.0)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        // account.append_limit_order(o, order_margin);
        // assert_eq!(account.active_limit_orders().len(), 1);
        // assert_eq!(account.margin().wallet_balance(), base!(1.0));
        // assert_eq!(account.margin().position_margin(), base!(0.0));

        // account.cancel_order(0).unwrap();
        // assert_eq!(account.active_limit_orders().len(), 0);
        // assert_eq!(account.margin().wallet_balance(), base!(1.0));
        // assert_eq!(account.margin().position_margin(), base!(0.0));
    }

    #[test]
    fn account_cancel_order_by_user_id() {
        if let Err(_) = pretty_env_logger::try_init() {}

        todo!();
        // let mut account = Account::new(NoAccountTracker::default(), leverage!(1.0), base!(1.0));
        // let mut validator = Validator::new(fee!(0.0), fee!(0.0), 100);
        // validator.update(quote!(100.0), quote!(100.1));

        // let mut o = Order::limit(Side::Buy, quote!(100.0), quote!(50.0)).unwrap();
        // o.set_user_order_id(1000);
        // let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        // account.append_limit_order(o, order_margin);
        // assert!(!account.active_limit_orders().is_empty());

        // account.cancel_order_by_user_id(1000).unwrap();
        // assert!(account.active_limit_orders.is_empty());
    }

    #[test]
    fn account_cancel_all_orders() {
        todo!()
        // let mut account = Account::new(NoAccountTracker::default(), leverage!(1.0), quote!(1000.0));
        // let mut validator = Validator::new(fee!(0.0), fee!(0.0), 100);
        // validator.update(quote!(900.0), quote!(901.0));

        // let o = Order::limit(Side::Buy, quote!(900.0), base!(0.45)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        // account.append_limit_order(o, order_margin);
        // assert_eq!(account.active_limit_orders().len(), 1);

        // assert_eq!(account.margin().wallet_balance(), quote!(1000.0));
        // assert_eq!(account.margin().position_margin(), quote!(0.0));
        // assert_eq!(account.margin().order_margin(), quote!(405.0));
        // assert_eq!(account.margin().available_balance(), quote!(595.0));

        // account.cancel_all_orders();
        // assert_eq!(account.active_limit_orders().len(), 0);
        // assert_eq!(account.margin().wallet_balance(), quote!(1000.0));
        // assert_eq!(account.margin().position_margin(), quote!(0.0));
        // assert_eq!(account.margin().order_margin(), quote!(0.0));
        // assert_eq!(account.margin().available_balance(), quote!(1000.0));
    }

    #[test]
    fn account_change_position_inverse_future() {
        todo!()
        // let mut acc = Account::new(NoAccountTracker::default(), leverage!(1.0), base!(1.0));

        // acc.change_position(Side::Buy, quote!(100.0), quote!(200.0), 0);
        // assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        // assert_eq!(acc.margin().position_margin(), base!(0.5));
        // assert_eq!(acc.margin().order_margin(), base!(0.0));
        // assert_eq!(acc.margin().available_balance(), base!(0.5));
        // assert_eq!(acc.position().size(), quote!(100.0));
        // assert_eq!(acc.position().entry_price(), quote!(200.0));
        // assert_eq!(acc.position().leverage(), leverage!(1.0));
        // assert_eq!(acc.position().unrealized_pnl(), base!(0.0));

        // acc.change_position(Side::Sell, quote!(100.0), quote!(200.0), 0);
        // assert_eq!(acc.position().size(), quote!(0.0));
        // assert_eq!(acc.position().entry_price(), quote!(200.0));
        // assert_eq!(acc.position().leverage(), leverage!(1.0));
        // assert_eq!(acc.position().unrealized_pnl(), base!(0.0));
        // assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        // assert_eq!(acc.margin().position_margin(), base!(0.0));
        // assert_eq!(acc.margin().order_margin(), base!(0.0));
        // assert_eq!(acc.margin().available_balance(), base!(1.0));

        // acc.change_position(Side::Sell, quote!(100.0), quote!(200.0), 0);
        // assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        // assert_eq!(acc.margin().position_margin(), base!(0.5));
        // assert_eq!(acc.margin().order_margin(), base!(0.0));
        // assert_eq!(acc.margin().available_balance(), base!(0.5));
        // assert_eq!(acc.position().size(), quote!(-100.0));
        // assert_eq!(acc.position().entry_price(), quote!(200.0));
        // assert_eq!(acc.position().leverage(), leverage!(1.0));
        // assert_eq!(acc.position().unrealized_pnl(), base!(0.0));

        // acc.change_position(Side::Buy, quote!(150.0), quote!(200.0), 0);
        // assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        // assert_eq!(acc.margin().position_margin(), base!(0.25));
        // assert_eq!(acc.margin().order_margin(), base!(0.0));
        // assert_eq!(acc.margin().available_balance(), base!(0.75));
        // assert_eq!(acc.position().size(), quote!(50.0));
        // assert_eq!(acc.position().entry_price(), quote!(200.0));
        // assert_eq!(acc.position().leverage(), leverage!(1.0));
        // assert_eq!(acc.position().unrealized_pnl(), base!(0.0));

        // acc.change_position(Side::Sell, quote!(25.0), quote!(200.0), 0);
        // assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        // assert_eq!(acc.margin().position_margin(), base!(0.125));
        // assert_eq!(acc.margin().order_margin(), base!(0.0));
        // assert_eq!(acc.margin().available_balance(), base!(0.875));
        // assert_eq!(acc.position().size(), quote!(25.0));
        // assert_eq!(acc.position().entry_price(), quote!(200.0));
        // assert_eq!(acc.position().leverage(), leverage!(1.0));
        // assert_eq!(acc.position().unrealized_pnl(), base!(0.0));
    }

    #[test]
    fn account_change_position_linear_futures() {
        todo!()
        // let mut acc = Account::new(NoAccountTracker::default(), leverage!(1.0), quote!(1000.0));

        // acc.change_position(Side::Buy, base!(0.5), quote!(100.0), 0);
        // assert_eq!(acc.margin().wallet_balance(), quote!(1000.0));
        // assert_eq!(acc.margin().position_margin(), quote!(50.0));
        // assert_eq!(acc.margin().order_margin(), quote!(0.0));
        // assert_eq!(acc.margin().available_balance(), quote!(950.0));
        // assert_eq!(acc.position().size(), base!(0.5));
        // assert_eq!(acc.position().entry_price(), quote!(100.0));
        // assert_eq!(acc.position().leverage(), leverage!(1.0));
        // assert_eq!(acc.position().unrealized_pnl(), quote!(0.0));
    }

    #[test]
    fn account_open_limit_buy_size() {
        todo!()
        // let mut acc = Account::new(NoAccountTracker::default(), leverage!(1.0), quote!(100.0));
        // let mut validator = Validator::new(fee!(0.0), fee!(0.0), 100);
        // validator.update(quote!(100.0), quote!(100.1));

        // let o = Order::limit(Side::Buy, quote!(100.0), base!(0.5)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size(), base!(0.5));

        // let mut o = Order::limit(Side::Buy, quote!(100.0), base!(0.5)).unwrap();
        // o.set_id(1);
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size(), base!(1.0));

        // let mut o = Order::limit(Side::Sell, quote!(100.0), base!(0.5)).unwrap();
        // o.set_id(2);
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_buy_size(), base!(1.0));

        // acc.cancel_order(0).unwrap();
        // assert_eq!(acc.open_limit_buy_size(), base!(1.0));
    }

    #[test]
    fn account_open_limit_sell_size() {
        todo!()
        // let mut acc = Account::new(NoAccountTracker::default(), leverage!(1.0), quote!(100.0));
        // let mut validator = Validator::new(fee!(0.0), fee!(0.0), 100);
        // validator.update(quote!(100.0), quote!(100.1));

        // let o = Order::limit(Side::Sell, quote!(100.0), base!(0.5)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_sell_size(), base!(0.5));

        // let mut o = Order::limit(Side::Sell, quote!(100.0), base!(0.5)).unwrap();
        // o.set_id(1);
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_sell_size(), base!(1.0));

        // let o = Order::limit(Side::Buy, quote!(100.0), base!(0.5)).unwrap();
        // let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        // acc.append_limit_order(o, order_margin);
        // assert_eq!(acc.open_limit_sell_size(), base!(1.0));
    }
}
