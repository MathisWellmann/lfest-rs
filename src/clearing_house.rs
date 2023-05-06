//! A clearinghouse clears and settles all trades and collects margin

use fpdec::Decimal;

use crate::{
    prelude::{Account, AccountTracker},
    quote,
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
    pub(crate) fn settle_funding_period(&mut self, mark_value: M, funding_rate: Decimal) {
        todo!()
    }

    /// Tries to increase a long (or neutral) position of the account.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount by which to incrase.
    /// `price`: The execution price.
    ///
    /// # Returns:
    /// If Err, then there was not enough available balance.
    /// Ok if successfull.
    pub(crate) fn try_increase_long(
        &mut self,
        account: &mut Account<M::PairedCurrency>,
        amount: M::PairedCurrency,
        price: QuoteCurrency,
    ) {
        // let value = amount.convert(price);
        // account.position.increase_long(amount, price);
        todo!()
    }

    /// Decrease a long position, realizing pnl while doing so.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to decrease by, must be smaller or equal to the existing long `size`.
    /// `price`: The execution price, determines the pnl.
    ///
    /// # Returns:
    /// If Err the transaction failed, but due to the atomic nature of this call nothing happens.
    pub(crate) fn try_decrease_long(
        &mut self,
        account: &mut Account<M::PairedCurrency>,
        amount: M::PairedCurrency,
        price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        debug_assert!(amount > M::PairedCurrency::new_zero());
        debug_assert!(price > quote!(0));

        todo!();
        // let value = amount.convert(price);
        // let pnl = self.position.decrease_long(amount, price)?;

        // let fees = value * fee;
        // // Fee just vanishes as there is no one to benefit from the fee.
        // let net_pnl = pnl - fees;
        // todo!("realize pnl or return it");
        // self.realize_pnl(net_pnl, ts_ns);
    }

    /// Tries to increase a short (or neutral) position of the account.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount by which to incrase.
    /// `price`: The execution price.
    ///
    /// # Returns:
    /// If Err, then there was not enough available balance.
    /// Ok if successfull.
    pub(crate) fn try_increase_short(
        &mut self,
        account: &mut Account<M::PairedCurrency>,
        amount: M::PairedCurrency,
        price: QuoteCurrency,
    ) {
        debug_assert!(amount > M::PairedCurrency::new_zero());
        debug_assert!(price > quote!(0));

        todo!();
        // self.position.increase_short(amount, price);
    }

    /// Decrease a short position, realizing pnl while doing so.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to decrease by, must be smaller or equal to the existing long `size`.
    /// `price`: The execution price, determines the pnl.
    ///
    /// # Returns:
    /// If Err the transaction failed, but due to the atomic nature of this call nothing happens.
    pub(crate) fn try_decrease_short(
        &mut self,
        account: &mut Account<M::PairedCurrency>,
        amount: M::PairedCurrency,
        price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) {
        debug_assert!(amount > M::PairedCurrency::new_zero());
        debug_assert!(price > quote!(0));

        // let pnl = self.position.decrease_short(amount, price)?;
        // let fees = amount.convert(price) * fee;
        // // Fee just vanishes as there is no one to benefit from the fee.
        // let net_pnl = pnl - fees;
        todo!("realize profit");
        // self.realize_pnl(net_pnl, ts_ns);
    }

    /// Turn a long position into a short one by,
    /// 0. ensuring there is enough balance, all things considered.
    /// 1. reducing the *existing* long position.
    /// 2. entering a new short
    pub(crate) fn try_turn_around_long(
        &mut self,
        account: &mut Account<M::PairedCurrency>,
        amount: M::PairedCurrency,
        price: QuoteCurrency,
    ) {
        debug_assert!(amount > M::PairedCurrency::new_zero());
        debug_assert!(price > quote!(0));

        todo!()
    }

    /// Turn a short position into a long one by,
    /// 0. ensuring there is enough balance, all things considered.
    /// 1. reducing the *existing* long position.
    /// 2. entering a new short
    pub(crate) fn try_turn_around_short(
        &mut self,
        account: &mut Account<M::PairedCurrency>,
        amount: M::PairedCurrency,
        price: QuoteCurrency,
    ) {
        debug_assert!(amount > M::PairedCurrency::new_zero());
        debug_assert!(price > quote!(0));

        todo!()
    }
}
