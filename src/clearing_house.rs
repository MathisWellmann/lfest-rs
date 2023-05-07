//! A clearinghouse clears and settles all trades and collects margin

use fpdec::Decimal;

use crate::{
    prelude::{Account, AccountTracker},
    types::{Currency, Fee, MarginCurrency, QuoteCurrency},
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
pub struct ClearingHouse<A, M> {
    /// Keeps track of all trades of the `Account`.
    account_tracker: A,
    _margin_curr: std::marker::PhantomData<M>,
}

impl<A, M> ClearingHouse<A, M>
where
    A: AccountTracker<M>,
    M: Currency + MarginCurrency,
{
    /// Create a new instance with a user account
    pub(crate) fn new(account_tracker: A) -> Self {
        Self {
            account_tracker,
            _margin_curr: Default::default(),
        }
    }

    /// The funding period for perpetual futures has ended.
    /// Funding = `mark_value` * `funding_rate`.
    /// `mark_value` is denoted in the margin currency.
    /// If the funding rate is positive, longs pay shorts.
    /// Else its the otherway around.
    /// TODO: not used but may be in the future.
    pub(crate) fn settle_funding_period(&mut self, mark_value: M, funding_rate: Decimal) {
        todo!()
    }

    /// Return a reference to the `AccountTracker` for performance statistics.
    #[inline(always)]
    pub(crate) fn account_tracker(&self) -> &A {
        &self.account_tracker
    }

    /// Settlement referes to the actual transfer of funds or assets between the buyer and seller to fulfill the trade.
    /// As the `ClearingHouse` is the central counterparty to every trade,
    /// it is the buyer of every sell order,
    /// and the seller of every buy order.
    ///
    /// # Arguments:
    /// `quantity`: The number of contract traded, where a negative number indicates a sell.
    /// `fill_price`: The execution price of the trade
    /// `fee`: The fee fraction for this type of order settlement.
    ///
    pub(crate) fn settle_filled_order(
        &mut self,
        account: &mut Account<M>,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        if quantity > M::PairedCurrency::new_zero() {
            self.settle_buy_order(account, quantity, fill_price, fee, ts_ns);
        } else {
            self.settle_sell_order(account, quantity.abs(), fill_price, fee, ts_ns);
        }
    }

    fn settle_buy_order(
        &mut self,
        account: &mut Account<M>,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        let notional_value = quantity.convert(fill_price);
        let fee = notional_value * fee;
        account.wallet_balance -= fee;
        self.account_tracker.log_fee(fee);

        if account.position.size() >= M::PairedCurrency::new_zero() {
            account.position.increase_long(quantity, fill_price);
        } else {
            if quantity.into_negative() >= account.position.size {
                // Strictly decrease the short position
                let rpnl = account.position.decrease_short(quantity, fill_price);
                account.wallet_balance += rpnl;
                self.account_tracker.log_rpnl(rpnl, ts_ns);
            } else {
                let new_long_size = quantity - account.position.size().abs();

                // decrease the short first
                let rpnl = account
                    .position
                    .decrease_short(account.position.size().abs(), fill_price);
                account.wallet_balance += rpnl;
                self.account_tracker.log_rpnl(rpnl, ts_ns);

                // also open a long
                account.position.open_position(new_long_size, fill_price);
            }
        }
    }

    fn settle_sell_order(
        &mut self,
        account: &mut Account<M>,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        let notional_value = quantity.convert(fill_price);
        let fee = notional_value * fee;
        account.wallet_balance -= fee;
        self.account_tracker.log_fee(fee);

        if account.position.size() > M::PairedCurrency::new_zero() {
            if quantity <= account.position.size() {
                // Decrease the long only
                let rpnl = account.position.decrease_long(quantity, fill_price);
                account.wallet_balance += rpnl;
                self.account_tracker.log_rpnl(rpnl, ts_ns);
            } else {
                let new_short_size = quantity - account.position.size();

                // Close the long
                let rpnl = account
                    .position
                    .decrease_long(account.position.size(), fill_price);

                account.wallet_balance += rpnl;
                self.account_tracker.log_rpnl(rpnl, ts_ns);

                // Open a short as well
                account
                    .position
                    .open_position(new_short_size.into_negative(), fill_price);
            }
        } else {
            // Increase short position
            account.position.increase_short(quantity, fill_price);
        }
    }
}
