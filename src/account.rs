use hashbrown::HashMap;

use crate::{
    limit_order_margin::order_margin,
    quote,
    utils::{max, min},
    AccountTracker, Currency, Error, Fee, FuturesTypes, Leverage, Margin, Order, Position,
    QuoteCurrency, Result, Side,
};

#[derive(Debug, Clone)]
/// The users account
/// Generic over:
/// A: AccountTracker,
/// S: The `Currency` representing the order quantity
/// B: Balance type
pub struct Account<A, S>
where S: Currency
{
    account_tracker: A,
    futures_type: FuturesTypes,
    margin: Margin<S::PairedCurrency>,
    position: Position<S>,
    active_limit_orders: HashMap<u64, Order<S>>,
    lookup_id_from_user_order_id: HashMap<u64, u64>,
    executed_orders: Vec<Order<S>>,
    // used for calculating hedged order size for order margin calculation
    open_limit_buy_size: S,
    open_limit_sell_size: S,
    // TODO: remove following two fields
    min_limit_buy_price: QuoteCurrency,
    max_limit_sell_price: QuoteCurrency,
}

impl<A, S> Account<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
{
    // TODO: make sure to eliminate the `futures_type`, to infer it based on the
    // starting_balance type
    pub(crate) fn new(
        account_tracker: A,
        leverage: Leverage,
        starting_balance: S::PairedCurrency,
        futures_type: FuturesTypes,
    ) -> Self {
        let position = Position::new_init(leverage);
        let margin = Margin::new_init(starting_balance);

        Self {
            account_tracker,
            futures_type,
            margin,
            position,
            active_limit_orders: HashMap::new(),
            lookup_id_from_user_order_id: HashMap::new(),
            executed_orders: vec![],
            open_limit_buy_size: S::new_zero(),
            open_limit_sell_size: S::new_zero(),
            min_limit_buy_price: quote!(0.0),
            max_limit_sell_price: quote!(0.0),
        }
    }

    /// Update the accounts state for the newest price data
    pub(crate) fn update(&mut self, price: &QuoteCurrency, trade_timestamp: u64) {
        let upnl = self.futures_type.pnl(self.position.entry_price(), price, self.position.size());
        self.account_tracker.update(trade_timestamp, *price, upnl);

        self.position.update_state(price, self.futures_type);
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
    #[inline(always)]
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

    /// Cancel an active order
    /// returns Some order if successful with given order_id
    pub fn cancel_order(&mut self, order_id: u64) -> Result<Order<S>> {
        debug!("cancel_order: {}", order_id);
        let removed_order = match self.active_limit_orders.remove(&order_id) {
            None => return Err(Error::OrderIdNotFound),
            Some(o) => o,
        };

        // re compute min and max prices for open orders
        self.min_limit_buy_price = quote!(0.0);
        self.max_limit_sell_price = quote!(0.0);
        for (_, o) in self.active_limit_orders.iter() {
            let limit_price = o.limit_price().unwrap();
            match o.side() {
                Side::Buy => {
                    if self.min_limit_buy_price.is_zero() {
                        self.min_limit_buy_price = limit_price;
                        continue;
                    }
                    if limit_price < self.min_limit_buy_price {
                        self.min_limit_buy_price = limit_price;
                    }
                }
                Side::Sell => {
                    if self.max_limit_sell_price.is_zero() {
                        self.max_limit_sell_price = limit_price;
                        continue;
                    }
                    if limit_price > self.max_limit_sell_price {
                        self.max_limit_sell_price = limit_price;
                    }
                }
            }
        }
        self.account_tracker.log_limit_order_cancellation();

        Ok(removed_order)
    }

    /// Cancel an active order based on the user_order_id of an Order
    ///
    /// # Returns:
    /// the cancelled order if successfull, error when the `user_order_id` is
    /// not found
    #[inline]
    pub fn cancel_order_by_user_id(&mut self, user_order_id: u64) -> Result<Order<S>> {
        debug!("cancel_order_by_user_id: user_order_id: {}", user_order_id);
        let id: u64 = match self.lookup_id_from_user_order_id.remove(&user_order_id) {
            None => return Err(Error::UserOrderIdNotFound),
            Some(id) => id,
        };
        self.cancel_order(id)
    }

    /// Cancel all active orders
    #[inline]
    pub fn cancel_all_orders(&mut self) {
        debug!("cancel_all_orders");

        self.margin.set_order_margin(S::PairedCurrency::new_zero());
        self.open_limit_buy_size = S::new_zero();
        self.open_limit_sell_size = S::new_zero();
        self.min_limit_buy_price = quote!(0.0);
        self.max_limit_sell_price = quote!(0.0);
        self.active_limit_orders.clear();
    }

    /// Cumulative open limit order size of buy orders
    #[inline(always)]
    pub fn open_limit_buy_size(&self) -> S {
        self.open_limit_buy_size
    }

    /// Cumulative
    #[inline(always)]
    pub fn open_limit_sell_size(&self) -> S {
        self.open_limit_sell_size
    }

    /// Append a new limit order as active order
    pub(crate) fn append_limit_order(&mut self, order: Order<S>, order_margin: S::PairedCurrency) {
        debug!("append_limit_order: order: {:?}, order_margin: {}", order, order_margin);

        let limit_price = order.limit_price().unwrap();
        match order.side() {
            Side::Buy => {
                self.open_limit_buy_size += order.quantity();
                if self.min_limit_buy_price.is_zero() {
                    self.min_limit_buy_price = limit_price;
                }
                if limit_price < self.min_limit_buy_price {
                    self.min_limit_buy_price = limit_price;
                }
            }
            Side::Sell => {
                self.open_limit_sell_size += order.quantity();
                if self.max_limit_sell_price.is_zero() {
                    self.max_limit_sell_price = limit_price;
                }
                if limit_price > self.max_limit_sell_price {
                    self.max_limit_sell_price = limit_price;
                }
            }
        }

        let new_om = self.margin.order_margin() + order_margin;
        self.margin.set_order_margin(new_om);

        self.account_tracker.log_limit_order_submission();
        let order_id = order.id();
        let user_order_id = order.user_order_id().clone();
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
                self.lookup_id_from_user_order_id.insert(user_order_id, order_id);
            }
        };
    }

    /// Remove the assigned order margin for a given order
    pub(crate) fn remove_executed_order_from_order_margin_calculation(
        &mut self,
        exec_order: &Order<S>,
    ) {
        match exec_order.side() {
            Side::Buy => self.open_limit_buy_size -= exec_order.quantity(),
            Side::Sell => self.open_limit_sell_size -= exec_order.quantity(),
        }
        debug!(
            "remove_executed_order_from_order_margin_calculation: olbs {}, olss: {}",
            self.open_limit_buy_size, self.open_limit_sell_size
        );

        self.active_limit_orders.remove(&exec_order.id()).unwrap();

        // re-calculate min and max price
        self.min_limit_buy_price = if self.active_limit_orders.is_empty() {
            quote!(0.0)
        } else {
            self.active_limit_orders
                .iter()
                .filter(|(_, o)| o.side() == Side::Buy)
                .map(|(_, o)| o.limit_price().unwrap())
                .fold(quote!(f64::MAX), min)
        };
        self.max_limit_sell_price = if self.active_limit_orders.is_empty() {
            quote!(0.0)
        } else {
            self.active_limit_orders
                .iter()
                .filter(|(_, o)| o.side() == Side::Sell)
                .map(|(_, o)| o.limit_price().unwrap())
                .fold(quote!(f64::MIN), max)
        };

        // set this to 0.0 temporarily and it will be properly assigned at the end of
        // limit order execution
        self.margin.set_order_margin(S::PairedCurrency::new_zero());
    }

    /// Finalize an executed limit order
    pub(crate) fn finalize_limit_order(&mut self, mut exec_order: Order<S>, fee_maker: &Fee) {
        exec_order.mark_executed();

        self.account_tracker.log_limit_order_fill();
        self.executed_orders.push(exec_order);

        let new_om = order_margin(
            self.active_limit_orders.values().cloned(),
            self.position.size(),
            self.futures_type,
            self.position.leverage(),
            fee_maker,
        );
        self.margin.set_order_margin(new_om);
    }

    /// Reduce the account equity by a fee amount, measured in the margin
    /// currency
    pub(crate) fn deduce_fees(&mut self, fee: S::PairedCurrency) {
        debug!("account: deduce_fees: deducing {} in fees", fee);

        self.account_tracker.log_fee(fee);
        self.margin.change_balance(fee.into_negative());
    }

    /// Changes the position by a given delta while changing margin accordingly
    pub(crate) fn change_position(&mut self, side: Side, size: S, exec_price: QuoteCurrency) {
        debug!(
            "account: change_position(side: {:?}, size: {}, exec_price: {})",
            side, size, exec_price
        );
        let pos_size_delta = match side {
            Side::Buy => size,
            Side::Sell => size.into_negative(),
        };
        let rpnl = match side {
            Side::Buy => {
                if self.position.size() < S::new_zero() {
                    // pnl needs to be realized
                    if size > self.position.size().abs() {
                        self.futures_type.pnl(
                            self.position.entry_price(),
                            &exec_price,
                            self.position.size(),
                        )
                    } else {
                        self.futures_type.pnl(
                            self.position.entry_price(),
                            &exec_price,
                            size.into_negative(),
                        )
                    }
                } else {
                    S::PairedCurrency::new_zero()
                }
            }
            Side::Sell => {
                if self.position.size() > S::new_zero() {
                    // pnl needs to be realized
                    if size > self.position.size() {
                        self.futures_type.pnl(
                            self.position.entry_price(),
                            &exec_price,
                            self.position.size(),
                        )
                    } else {
                        self.futures_type.pnl(self.position.entry_price(), &exec_price, size)
                    }
                } else {
                    S::PairedCurrency::new_zero()
                }
            }
        };
        if rpnl != S::PairedCurrency::new_zero() {
            // first free up existing position margin if any
            let new_pos_margin = ((self.position().size() + pos_size_delta).abs()
                / S::new(self.position().leverage().inner().clone()))
            .convert(&self.position.entry_price());
            self.margin.set_position_margin(new_pos_margin);

            self.margin.change_balance(rpnl);
            self.account_tracker.log_rpnl(rpnl);
        }

        // change position
        self.position.change_size(&pos_size_delta, &exec_price, self.futures_type);

        // set position margin
        let pos_margin: S::PairedCurrency = (self.position.size().abs()
            / S::new(self.position.leverage().inner().clone()))
        .convert(&self.position.entry_price());
        self.margin.set_position_margin(pos_margin);

        // log change
        self.account_tracker.log_trade(side, &exec_price, &size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{account_tracker::NoAccountTracker, base, BaseCurrency, FuturesTypes, Validator};

    #[test]
    fn account_append_limit_order() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut acc = Account::new(NoAccountTracker::default(), 1.0, base!(1.0), futures_type);
        let mut validator = Validator::new(Fee(0.0), Fee(0.0), futures_type, 100);
        validator.update(quote!(100.0), quote!(101.0));

        let o = Order::limit(Side::Buy, quote!(100.0), quote!(25.0)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, quote!(25.0));
        assert_eq!(acc.open_limit_sell_size, quote!(0.0));
        assert_eq!(acc.min_limit_buy_price, quote!(100.0));
        assert_eq!(acc.max_limit_sell_price, quote!(0.0));
        assert_eq!(acc.margin().order_margin(), base!(0.25));
        assert_eq!(acc.margin().available_balance(), base!(0.75));

        let o = Order::limit(Side::Sell, quote!(100.0), quote!(25.0)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, quote!(25.0));
        assert_eq!(acc.open_limit_sell_size, quote!(25.0));
        assert_eq!(acc.min_limit_buy_price, quote!(100.0));
        assert_eq!(acc.max_limit_sell_price, quote!(100.0));
        assert_eq!(acc.margin().order_margin(), base!(0.25));
        assert_eq!(acc.margin().available_balance(), base!(0.75));

        let o = Order::limit(Side::Buy, quote!(90.0), quote!(25.0)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, quote!(50.0));
        assert_eq!(acc.open_limit_sell_size, quote!(25.0));
        assert_eq!(acc.min_limit_buy_price, quote!(90.0));
        assert_eq!(acc.max_limit_sell_price, quote!(100.0));
        // TODO: what is the proper test result here
        // assert_eq!(account.margin().order_margin(), 0.5278);
        // assert_eq!(account.margin().available_balance(), 0.75);

        let o = Order::limit(Side::Sell, quote!(110.0), quote!(25.0)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size, quote!(50.0));
        assert_eq!(acc.open_limit_sell_size, quote!(50.0));
        assert_eq!(acc.min_limit_buy_price, quote!(90.0));
        assert_eq!(acc.max_limit_sell_price, quote!(110.0));
        // assert_eq!(account.margin().order_margin(), 0.5278);
        // assert_eq!(account.margin().available_balance(), 0.75);
    }

    #[test]
    fn account_cancel_order() {
        let futures_type = FuturesTypes::Inverse;
        let mut account = Account::new(NoAccountTracker::default(), 1.0, base!(1.0), futures_type);
        let mut validator = Validator::new(Fee(0.0), Fee(0.0), futures_type, 100);
        validator.update(quote!(900.0), quote!(901.0));

        let o = Order::limit(Side::Buy, quote!(900.0), quote!(450.0)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        account.append_limit_order(o, order_margin);
        assert_eq!(account.active_limit_orders().len(), 1);
        assert_eq!(account.margin().wallet_balance(), base!(1.0));
        assert_eq!(account.margin().position_margin(), base!(0.0));

        account.cancel_order(0).unwrap();
        assert_eq!(account.active_limit_orders().len(), 0);
        assert_eq!(account.margin().wallet_balance(), base!(1.0));
        assert_eq!(account.margin().position_margin(), base!(0.0));
    }

    #[test]
    fn account_cancel_order_by_user_id() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let futures_type = FuturesTypes::Inverse;
        let mut account = Account::new(NoAccountTracker::default(), 1.0, base!(1.0), futures_type);
        let mut validator = Validator::new(Fee(0.0), Fee(0.0), futures_type, 100);
        validator.update(quote!(100.0), quote!(100.1));

        let mut o = Order::limit(Side::Buy, quote!(100.0), quote!(50.0)).unwrap();
        o.set_user_order_id(1000);
        let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        account.append_limit_order(o, order_margin);
        assert!(!account.active_limit_orders().is_empty());

        account.cancel_order_by_user_id(1000).unwrap();
        assert!(account.active_limit_orders.is_empty());
    }

    #[test]
    fn account_cancel_all_orders() {
        let futures_type = FuturesTypes::Inverse;
        let mut account = Account::new(NoAccountTracker::default(), 1.0, base!(1.0), futures_type);
        let mut validator = Validator::new(Fee(0.0), Fee(0.0), futures_type, 100);
        validator.update(quote!(900.0), quote!(901.0));

        let o = Order::limit(Side::Buy, quote!(900.0), quote!(450.0)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &account).unwrap();
        account.append_limit_order(o, order_margin);
        assert_eq!(account.active_limit_orders().len(), 1);
        assert_eq!(account.margin().wallet_balance(), base!(1.0));
        assert_eq!(account.margin().position_margin(), base!(0.0));
        assert_eq!(account.margin().order_margin(), base!(0.5));
        assert_eq!(account.margin().available_balance(), base!(0.5));

        account.cancel_all_orders();
        assert_eq!(account.active_limit_orders().len(), 0);
        assert_eq!(account.margin().wallet_balance(), base!(1.0));
        assert_eq!(account.margin().position_margin(), base!(0.0));
        assert_eq!(account.margin().order_margin(), base!(0.0));
        assert_eq!(account.margin().available_balance(), base!(1.0));
    }

    #[test]
    fn account_change_position_inverse_future() {
        let futures_type = FuturesTypes::Inverse;
        let mut acc = Account::new(NoAccountTracker::default(), 1.0, base!(1.0), futures_type);

        acc.change_position(Side::Buy, quote!(100.0), quote!(200.0));
        assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        assert_eq!(acc.margin().position_margin(), base!(0.5));
        assert_eq!(acc.margin().order_margin(), base!(0.0));
        assert_eq!(acc.margin().available_balance(), base!(0.5));
        assert_eq!(acc.position().size(), quote!(100.0));
        assert_eq!(acc.position().entry_price(), quote!(200.0));
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), base!(0.0));

        acc.change_position(Side::Sell, quote!(100.0), quote!(200.0));
        assert_eq!(acc.position().size(), quote!(0.0));
        assert_eq!(acc.position().entry_price(), quote!(200.0));
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), base!(0.0));
        assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        assert_eq!(acc.margin().position_margin(), base!(0.0));
        assert_eq!(acc.margin().order_margin(), base!(0.0));
        assert_eq!(acc.margin().available_balance(), base!(1.0));

        acc.change_position(Side::Sell, quote!(100.0), quote!(200.0));
        assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        assert_eq!(acc.margin().position_margin(), base!(0.5));
        assert_eq!(acc.margin().order_margin(), base!(0.0));
        assert_eq!(acc.margin().available_balance(), base!(0.5));
        assert_eq!(acc.position().size(), quote!(-100.0));
        assert_eq!(acc.position().entry_price(), quote!(200.0));
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), base!(0.0));

        acc.change_position(Side::Buy, quote!(150.0), quote!(200.0));
        assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        assert_eq!(acc.margin().position_margin(), base!(0.25));
        assert_eq!(acc.margin().order_margin(), base!(0.0));
        assert_eq!(acc.margin().available_balance(), base!(0.75));
        assert_eq!(acc.position().size(), quote!(50.0));
        assert_eq!(acc.position().entry_price(), quote!(200.0));
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), base!(0.0));

        acc.change_position(Side::Sell, quote!(25.0), quote!(200.0));
        assert_eq!(acc.margin().wallet_balance(), base!(1.0));
        assert_eq!(acc.margin().position_margin(), base!(0.125));
        assert_eq!(acc.margin().order_margin(), base!(0.0));
        assert_eq!(acc.margin().available_balance(), base!(0.875));
        assert_eq!(acc.position().size(), quote!(25.0));
        assert_eq!(acc.position().entry_price(), quote!(200.0));
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), base!(0.0));
    }

    #[test]
    fn account_change_position_linear_futures() {
        let futures_type = FuturesTypes::Linear;
        let mut acc = Account::new(NoAccountTracker::default(), 1.0, quote!(1000.0), futures_type);

        acc.change_position(Side::Buy, base!(0.5), quote!(100.0));
        assert_eq!(acc.margin().wallet_balance(), quote!(1000.0));
        assert_eq!(acc.margin().position_margin(), quote!(50.0));
        assert_eq!(acc.margin().order_margin(), quote!(0.0));
        assert_eq!(acc.margin().available_balance(), quote!(950.0));
        assert_eq!(acc.position().size(), base!(0.5));
        assert_eq!(acc.position().entry_price(), quote!(100.0));
        assert_eq!(acc.position().leverage(), 1.0);
        assert_eq!(acc.position().unrealized_pnl(), quote!(0.0));
    }

    #[test]
    fn account_open_limit_buy_size() {
        let futures_type = FuturesTypes::Linear;
        let mut acc = Account::new(NoAccountTracker::default(), 1.0, quote!(100.0), futures_type);
        let mut validator = Validator::new(Fee(0.0), Fee(0.0), futures_type, 100);
        validator.update(quote!(100.0), quote!(100.1));

        let o = Order::limit(Side::Buy, quote!(100.0), base!(0.5)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size(), base!(0.5));

        let mut o = Order::limit(Side::Buy, quote!(100.0), base!(0.5)).unwrap();
        o.set_id(1);
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size(), base!(1.0));

        let mut o = Order::limit(Side::Sell, quote!(100.0), base!(0.5)).unwrap();
        o.set_id(2);
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_buy_size(), base!(1.0));

        acc.cancel_order(0).unwrap();
        assert_eq!(acc.open_limit_buy_size(), base!(1.0));
    }

    #[test]
    fn account_open_limit_sell_size() {
        let futures_type = FuturesTypes::Linear;
        let mut acc = Account::new(NoAccountTracker::default(), 1.0, quote!(100.0), futures_type);
        let mut validator = Validator::new(Fee(0.0), Fee(0.0), futures_type, 100);
        validator.update(quote!(100.0), quote!(100.1));

        let o = Order::limit(Side::Sell, quote!(100.0), base!(0.5)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_sell_size(), base!(0.5));

        let mut o = Order::limit(Side::Sell, quote!(100.0), base!(0.5)).unwrap();
        o.set_id(1);
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_sell_size(), base!(1.0));

        let o = Order::limit(Side::Buy, quote!(100.0), base!(0.5)).unwrap();
        let order_margin = validator.validate_limit_order(&o, &acc).unwrap();
        acc.append_limit_order(o, order_margin);
        assert_eq!(acc.open_limit_sell_size(), base!(1.0));
    }
}
