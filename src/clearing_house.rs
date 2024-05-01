//! A clearinghouse clears and settles all trades and collects margin

use fpdec::Decimal;

use crate::{
    prelude::{Account, AccountTracker},
    types::{Currency, Fee, MarginCurrency, QuoteCurrency, Side},
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
pub struct ClearingHouse<A, M, UserOrderId> {
    _phanton: std::marker::PhantomData<(A, M, UserOrderId)>,
}

impl<A, M, UserOrderId> ClearingHouse<A, M, UserOrderId>
where
    A: AccountTracker<M>,
    M: Currency + MarginCurrency,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash,
{
    /// Create a new instance.
    pub(crate) fn new() -> Self {
        Self {
            _phanton: Default::default(),
        }
    }

    /// The funding period for perpetual futures has ended.
    /// Funding = `mark_value` * `funding_rate`.
    /// `mark_value` is denoted in the margin currency.
    /// If the funding rate is positive, longs pay shorts.
    /// Else its the otherway around.
    /// TODO: not used but may be in the future.
    #[allow(unused)]
    pub(crate) fn settle_funding_period(&mut self, _mark_value: M, _funding_rate: Decimal) {
        todo!("Support `settle_funding_period`")
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
        account: &mut Account<M, UserOrderId>,
        account_tracker: &mut A,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        let side = if quantity > M::PairedCurrency::new_zero() {
            Side::Buy
        } else {
            Side::Sell
        };
        account_tracker.log_trade(side, fill_price, quantity);

        if quantity > M::PairedCurrency::new_zero() {
            self.settle_buy_order(account, account_tracker, quantity, fill_price, fee, ts_ns);
        } else {
            self.settle_sell_order(
                account,
                account_tracker,
                quantity.abs(),
                fill_price,
                fee,
                ts_ns,
            );
        }
    }

    fn settle_buy_order(
        &mut self,
        account: &mut Account<M, UserOrderId>,
        account_tracker: &mut A,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        let notional_value = quantity.convert(fill_price);
        let fee = notional_value * fee;
        account.wallet_balance -= fee;
        account_tracker.log_fee(fee);

        if account.position.size() >= M::PairedCurrency::new_zero() {
            account.increase_long(quantity, fill_price);
        } else {
            // Position must be short
            if quantity.into_negative() >= account.position.size {
                // Strictly decrease the short position
                let rpnl = account.decrease_short(quantity, fill_price);
                account.wallet_balance += rpnl;
                account_tracker.log_rpnl(rpnl - fee, ts_ns);
            } else {
                let new_long_size = quantity - account.position.size().abs();

                // decrease the short first
                let rpnl = account.decrease_short(account.position.size().abs(), fill_price);
                account.wallet_balance += rpnl;
                account_tracker.log_rpnl(rpnl - fee, ts_ns);

                // also open a long
                account.open_position(new_long_size, fill_price);
            }
        }
    }

    fn settle_sell_order(
        &mut self,
        account: &mut Account<M, UserOrderId>,
        account_tracker: &mut A,
        quantity: M::PairedCurrency,
        fill_price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        let notional_value = quantity.convert(fill_price);
        let fee = notional_value * fee;
        account.wallet_balance -= fee;
        account_tracker.log_fee(fee);

        if account.position.size() > M::PairedCurrency::new_zero() {
            if quantity <= account.position.size() {
                // Decrease the long only
                let rpnl = account.decrease_long(quantity, fill_price);
                account.wallet_balance += rpnl;
                account_tracker.log_rpnl(rpnl - fee, ts_ns);
            } else {
                let new_short_size = quantity - account.position.size();

                // Close the long
                let rpnl = account.decrease_long(account.position.size(), fill_price);

                account.wallet_balance += rpnl;
                account_tracker.log_rpnl(rpnl - fee, ts_ns);

                // Open a short as well
                account.open_position(new_short_size.into_negative(), fill_price);
            }
        } else {
            // Increase short position
            account.increase_short(quantity, fill_price);
        }
    }
}
