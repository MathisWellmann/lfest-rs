use crate::{
    quote, Account, AccountTracker, Config, Currency, FuturesTypes, MarketUpdate, NoAccountTracker,
    Order, OrderError, OrderType, QuoteCurrency, Side, Validator,
};

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange<A, S>
where S: Currency
{
    config: Config<S::PairedCurrency>,
    user_account: Account<A, S>,
    /// An infinitely liquid counterparty that can
    counterparty_account: Account<NoAccountTracker, S::PairedCurrency>,
    validator: Validator,
    bid: QuoteCurrency,
    ask: QuoteCurrency,
    next_order_id: u64,
    step: u64, // used for synchronizing orders
    high: QuoteCurrency,
    low: QuoteCurrency,
    current_ts: i64,
}

impl<A, S> Exchange<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<S::PairedCurrency>) -> Self {
        let account = Account::new(
            account_tracker,
            config.leverage(),
            config.starting_balance(),
            config.futures_type(),
        );
        let validator =
            Validator::new(config.fee_maker(), config.fee_taker(), config.futures_type());

        let counterparty_account = Account::new(
            NoAccountTracker::default(),
            1.0,
            (f64::MAX).into(),
            config.futures_type(),
        );
        Self {
            config,
            user_account: account,
            counterparty_account,
            validator,
            bid: quote!(0.0),
            ask: quote!(0.0),
            next_order_id: 0,
            step: 0,
            high: quote!(0.0),
            low: quote!(0.0),
            current_ts: 0,
        }
    }

    /// Return a reference to current exchange config
    #[inline(always)]
    pub fn config(&self) -> &Config<S::PairedCurrency> {
        &self.config
    }

    /// Return the bid price
    #[inline(always)]
    pub fn bid(&self) -> QuoteCurrency {
        self.bid
    }

    /// Return the ask price
    #[inline(always)]
    pub fn ask(&self) -> QuoteCurrency {
        self.ask
    }

    /// Return the current time step
    #[inline(always)]
    pub fn current_step(&self) -> u64 {
        self.step
    }

    /// Return a reference to Account
    #[inline(always)]
    pub fn account(&self) -> &Account<A, S> {
        &self.user_account
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account<A, S> {
        &mut self.user_account
    }

    /// Set the account, use carefully
    #[inline(always)]
    pub fn set_account(&mut self, account: Account<A, S>) {
        self.user_account = account
    }

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// timestamp: Is used in the AccountTracker A
    ///     and if setting order timestamps is enabled in the config.
    ///     So can be set to 0 if not required
    /// market_update: Newest market information
    ///
    /// ### Returns:
    /// executed orders
    /// true if position has been liquidated
    #[must_use]
    pub fn update_state(
        &mut self,
        timestamp: u64,
        market_update: MarketUpdate,
    ) -> (Vec<Order<S>>, bool) {
        // TODO: enforce bid ask spread
        // TODO: add price filters with precision, possibly by changing from f64 to some
        // decimal lib
        match market_update {
            MarketUpdate::Bba {
                bid,
                ask,
            } => {
                self.bid = bid;
                self.ask = ask;
                self.high = ask;
                self.low = bid;
            }
            MarketUpdate::Candle {
                bid,
                ask,
                high,
                low,
            } => {
                self.bid = bid;
                self.ask = ask;
                self.high = high;
                self.low = low;
            }
        }
        self.current_ts = timestamp as i64;

        self.validator.update(self.bid, self.ask);

        if self.check_liquidation() {
            self.liquidate();
            return (vec![], true);
        }

        self.check_orders();

        self.user_account.update((self.bid + self.ask) / 2.0, timestamp);

        self.step += 1;

        (self.user_account.executed_orders(), false)
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: Order<S>) -> Result<Order<S>, OrderError> {
        debug!("submit_order: {:?}", order);

        // assign unique order id
        order.set_id(self.next_order_id());

        if self.config.set_order_timestamps() {
            order.set_timestamp(self.current_ts);
        }

        match order.order_type() {
            OrderType::Market => {
                // immediately execute market order
                self.validator.validate_market_order(&order, &self.user_account)?;
                self.execute_market(order.side(), order.size());
                order.executed = true;

                Ok(order)
            }
            _ => {
                let order_margin =
                    self.validator.validate_limit_order(&order, &self.user_account)?;
                self.user_account.append_limit_order(order, order_margin);

                Ok(order)
            }
        }
    }

    /// Check if a liquidation event should occur
    fn check_liquidation(&mut self) -> bool {
        // TODO: check_liquidation
        // TODO: test check_liquidation

        false
    }

    /// Execute a market order
    fn execute_market(&mut self, side: Side, amount: S) {
        debug!("exchange: execute_market: side: {:?}, amount: {}", side, amount);

        let price = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        let fee_of_size = amount.fee_portion(self.config.fee_taker());
        let fee_margin = fee_of_size.convert(price);

        self.user_account.change_position(side, amount, price);
        self.user_account.deduce_fees(fee_margin);
    }

    /// Execute a limit order, once triggered
    fn execute_limit(&mut self, o: Order<S>) {
        debug!("execute_limit: {:?}", o);

        let price = o.limit_price().unwrap();

        self.user_account.remove_executed_order_from_order_margin_calculation(&o);

        self.user_account.change_position(o.side(), o.size(), price);

        let mut fee_of_size = o.size().fee_portion(self.config.fee_maker());
        let fee_margin = fee_of_size.convert(price);

        self.user_account.deduce_fees(fee_margin);
        self.user_account.finalize_limit_order(o, self.config.fee_maker());
    }

    /// Perform a liquidation of the account
    fn liquidate(&mut self) {
        // TODO: better liquidate
        debug!("liquidating");
        if self.user_account.position().size() > S::new_zero() {
            self.execute_market(Side::Sell, self.user_account.position().size());
        } else {
            self.execute_market(Side::Buy, self.user_account.position().size().abs());
        }
    }

    /// Check if any active orders have been triggered by the most recent price
    /// action method is called after new external data has been consumed
    fn check_orders(&mut self) {
        let keys: Vec<u64> =
            self.user_account.active_limit_orders().iter().map(|(i, _)| *i).collect();
        for i in keys {
            self.handle_limit_order(i);
        }
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_id: u64) {
        let o: Order<S> = *self
            .user_account
            .active_limit_orders()
            .get(&order_id)
            .expect("This order should be in HashMap for active limit orders");
        debug!("handle_limit_order: o: {:?}", o);
        let limit_price = o.limit_price().unwrap();
        match o.side() {
            Side::Buy => {
                // use candle information to specify execution
                if self.low < limit_price {
                    // this would be a guaranteed fill no matter the queue position in orderbook
                    self.execute_limit(o)
                }
            }
            Side::Sell => {
                // use candle information to specify execution
                if self.high > limit_price {
                    // this would be a guaranteed fill no matter the queue position in orderbook
                    self.execute_limit(o)
                }
            }
        }
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> u64 {
        self.next_order_id += 1;
        self.next_order_id - 1
    }
}
