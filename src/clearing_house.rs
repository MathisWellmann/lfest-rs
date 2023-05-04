//! A clearinghouse clears and settles all trades and collects margin

use fpdec::Decimal;

use crate::{
    prelude::{Account, AccountTracker, OrderError},
    quote,
    risk_engine::RiskEngine,
    types::{Currency, MarginCurrency, Order, OrderType, QuoteCurrency, Side},
};

/// A clearing house acts as an intermediary in futures transactions.
/// It guarantees the performance of the parties to each transaction.
/// The main task of the clearing house is to keep track of all the transactions
/// that take place, so that at can calculate the net position of each account.
///
/// If in total the transactions have lost money,
/// the account is required to provide variation margin to the exchange clearing
/// house. If there has been a gain on the transactions, the account receives
/// variation margin from the clearing house.
#[derive(Debug, Clone)]
pub struct ClearingHouse<A, S, R>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    /// Manages the risk if positions.
    risk_engine: R,
    /// Keeps track of all trades of the `Account`.
    account_tracker: A,
    /// The actual user of the exchange
    user_account: Account<S>,
    next_order_id: u64,
}

impl<A, S, R> ClearingHouse<A, S, R>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
    S::PairedCurrency: MarginCurrency,
    R: RiskEngine<S::PairedCurrency>,
{
    /// Create a new instance with a user account
    pub(crate) fn new(risk_engine: R, account_tracker: A, user_account: Account<S>) -> Self {
        Self {
            risk_engine,
            account_tracker,
            user_account,
            next_order_id: 0,
        }
    }

    /// The margin accounts are adjusted to reflect investors gain or loss.
    pub(crate) fn mark_to_market(&mut self, mark_price: QuoteCurrency) {
        // let position_value = self.user_account.position().size().convert(mark_price);

        todo!()
    }

    /// The funding period for perpetual futures has ended.
    /// Funding = `mark_value` * `funding_rate`.
    /// `mark_value` is denoted in the margin currency.
    /// If the funding rate is positive, longs pay shorts.
    /// Else its the otherway around.
    /// TODO: not used but may be in the future.
    pub(crate) fn settle_funding_period(
        &mut self,
        mark_value: S::PairedCurrency,
        funding_rate: Decimal,
    ) {
        todo!()
    }

    /// Get a reference to the user account
    #[inline(always)]
    pub(crate) fn user_account(&self) -> &Account<S> {
        &self.user_account
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account<S> {
        &mut self.user_account
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

        // assign unique order id
        order.set_id(self.next_order_id());
        order.set_timestamp(self.current_ts_ns);

        match order.order_type() {
            OrderType::Market => self.handle_market_order(order),
            OrderType::Limit => self.handle_new_limit_order(order),
        }
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

    fn handle_market_order(&mut self, mut order: Order<S>) -> Result<Order<S>, OrderError> {
        match order.side() {
            Side::Buy => {
                let price = self.ask;
                if self.user_account.position().size() >= S::new_zero() {
                    self.user_account
                        .try_increase_long(order.quantity(), price)
                        .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                } else {
                    if order.quantity() > self.user_account.position().size().abs() {
                        self.user_account
                            .try_turn_around_short(order.quantity(), price)
                            .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                    } else {
                        // decrease short and realize pnl.
                        self.user_account
                            .try_decrease_short(
                                order.quantity(),
                                price,
                                self.config.fee_taker(),
                                self.current_ts_ns,
                            )
                            .expect("Must be valid; qed");
                    }
                }
            }
            Side::Sell => {
                let price = self.bid;
                if self.user_account.position().size() >= S::new_zero() {
                    if order.quantity() > self.user_account.position().size() {
                        self.user_account
                            .try_turn_around_long(order.quantity(), price)
                            .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                    } else {
                        // decrease_long and realize pnl.
                        self.user_account
                            .try_decrease_long(
                                order.quantity(),
                                price,
                                self.config.fee_taker(),
                                self.current_ts_ns,
                            )
                            .expect("All inputs are valid; qed");
                    }
                } else {
                    self.user_account
                        .try_increase_short(order.quantity(), price)
                        .map_err(|_| OrderError::NotEnoughAvailableBalance)?;
                }
                todo!()
            }
        }
        order.mark_executed();

        Ok(order)
    }

    fn handle_new_limit_order(&mut self, order: Order<S>) -> Result<Order<S>, OrderError> {
        if self.user_account.num_active_limit_orders() >= self.config.max_num_open_orders() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_to_market() {
        todo!()
    }
}
