use fpdec::Decimal;

use crate::{
    account::Account,
    account_tracker::AccountTracker,
    config::Config,
    errors::{Error, OrderError},
    prelude::Side,
    quote,
    types::{Currency, MarginCurrency, MarketUpdate, Order, OrderType, QuoteCurrency},
};

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange<A, S>
where
    S: Currency + Default,
{
    config: Config<S::PairedCurrency>,
    user_account: Account<A, S>,
    bid: QuoteCurrency,
    ask: QuoteCurrency,
    next_order_id: u64,
    step: u64, // used for synchronizing orders
    high: QuoteCurrency,
    low: QuoteCurrency,
    // The current timestamp in nanoseconds
    current_ts_ns: i64,
}

impl<A, S> Exchange<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency + Default,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<S::PairedCurrency>) -> Self {
        let account = Account::new(
            account_tracker,
            config.leverage(),
            config.starting_balance(),
        );

        Self {
            config,
            user_account: account,
            bid: quote!(0.0),
            ask: quote!(0.0),
            next_order_id: 0,
            step: 0,
            high: quote!(0.0),
            low: quote!(0.0),
            current_ts_ns: 0,
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

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// executed orders
    /// true if position has been liquidated
    pub fn update_state(
        &mut self,
        timestamp_ns: u64,
        market_update: MarketUpdate,
    ) -> Result<(Vec<Order<S>>, bool), Error> {
        self.config
            .price_filter()
            .validate_market_update(&market_update)?;
        match market_update {
            MarketUpdate::Bba { bid, ask } => {
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
        self.current_ts_ns = timestamp_ns as i64;

        if self.check_liquidation() {
            self.liquidate();
            return Ok((vec![], true));
        }

        self.check_orders();

        self.user_account.update(self.bid, self.ask, timestamp_ns);

        self.step += 1;

        Ok((self.user_account.executed_orders(), false))
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: Order<S>) -> Result<Order<S>, OrderError> {
        trace!("submit_order: {:?}", order);

        // Basic checks
        self.config.quantity_filter().validate_order(&order)?;
        let mark_price = (self.bid + self.ask) / quote!(2);
        self.config
            .price_filter()
            .validate_order(&order, mark_price)?;

        if self.config.set_order_timestamps() {
            order.set_timestamp(self.current_ts_ns);
        }
        // assign unique order id
        order.set_id(self.next_order_id());

        match order.order_type() {
            OrderType::Market => self.handle_market_order(order),
            OrderType::Limit => self.handle_new_limit_order(order),
        }
    }

    /// Check if a liquidation event should occur
    fn check_liquidation(&mut self) -> bool {
        // TODO: check_liquidation
        // TODO: test check_liquidation

        false
    }

    /// Perform a liquidation of the account
    fn liquidate(&mut self) {
        todo!()
    }

    /// Check if any active orders have been triggered by the most recent price
    /// action method is called after new external data has been consumed
    fn check_orders(&mut self) {
        let keys = Vec::from_iter(
            self.user_account
                .active_limit_orders()
                .iter()
                .map(|(i, _)| *i),
        );
        for i in keys {
            self.handle_limit_order(i);
        }
    }

    fn handle_market_order(&mut self, order: Order<S>) -> Result<Order<S>, OrderError> {
        match order.side() {
            Side::Buy => {
                if self.account().position().size() >= S::new_zero() {
                    // increase_long (try reserve margin)
                    let margin_req = order.quantity().convert(self.ask) / self.config.leverage();
                    self.account()
                        .margin()
                        .lock_as_position_collateral(margin_req)
                        .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                    self.account()
                        .position()
                        .increase_long_position(order.quantity(), self.ask)
                        .expect("Increasing a position here must work; qed");
                } else {
                    // TODO: decrease_short (realize pnl)
                    // TODO: potentially increase_long (try reserve margin)
                }
                todo!()
            }
            Side::Sell => {
                if self.account().position().size() >= S::new_zero() {
                    // TODO: decrease_long (realize pnl)
                    // TODO: potentially increase_short (try reserve margin)
                } else {
                    // TODO: increase_short (try reserve margin)
                }
                todo!()
            }
        }
    }

    fn handle_new_limit_order(&mut self, order: Order<S>) -> Result<Order<S>, OrderError> {
        if self.account().num_active_limit_orders() >= self.config.max_num_open_orders() {
            return Err(OrderError::MaxActiveOrders);
        }
        // self.handle_limit_order(order_id);
        todo!()
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_id: u64) -> Result<(), OrderError> {
        todo!()
        // let o: Order<S> = self
        //     .user_account
        //     .active_limit_orders()
        //     .get(&order_id)
        //     .expect("This order should be in HashMap for active limit orders; qed")
        //     .clone();
        // debug!("handle_limit_order: o: {:?}", o);
        // let limit_price = o.limit_price().unwrap();
        // match o.side() {
        //     Side::Buy => {
        //         // use candle information to specify execution
        //         if self.low < limit_price {
        //             // this would be a guaranteed fill no matter the queue position in orderbook
        //             self.execute_limit(o)
        //         }
        //     }
        //     Side::Sell => {
        //         // use candle information to specify execution
        //         if self.high > limit_price {
        //             // this would be a guaranteed fill no matter the queue position in orderbook
        //             self.execute_limit(o)
        //         }
        //     }
        // }
    }

    #[inline(always)]
    fn next_order_id(&mut self) -> u64 {
        self.next_order_id += 1;
        self.next_order_id - 1
    }
}
