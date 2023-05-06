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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_try_increase_long() {
        let mut acc = mock_account();
        assert_eq!(
            acc.try_increase_long(base!(-1), quote!(100)),
            Err(Error::InvalidAmount)
        );
        assert_eq!(
            acc.try_increase_long(base!(1), quote!(-100)),
            Err(Error::InvalidPrice)
        );

        acc.try_increase_long(base!(1), quote!(100)).unwrap();
        assert_eq!(
            acc.margin,
            // + taker fee locked as position margin
            Margin::new(quote!(1000), quote!(100) + quote!(0.1), quote!(0)).unwrap()
        );
        assert_eq!(acc.position, Position::new(base!(1), quote!(100)),);

        // make sure it does not work with a short position
        acc.position = Position::new(base!(-1), quote!(100));
        assert_eq!(
            acc.try_increase_long(base!(0.5), quote!(100)),
            Err(Error::OpenShort)
        );
    }

    #[test]
    fn account_try_decrease_long() {
        let mut acc = mock_account();
        let fee = fee!(0.001);
        let ts_ns = 0;

        assert_eq!(
            acc.try_decrease_long(base!(-1), quote!(100), fee, ts_ns),
            Err(Error::InvalidAmount)
        );
        assert_eq!(
            acc.try_decrease_long(base!(1), quote!(-100), fee, ts_ns),
            Err(Error::InvalidPrice)
        );

        acc.try_increase_long(base!(1), quote!(100)).unwrap();
        assert_eq!(
            acc.try_decrease_long(base!(1.1), quote!(100), fee, ts_ns),
            Err(Error::InvalidAmount)
        );

        acc.try_decrease_long(base!(1), quote!(110), fee, ts_ns)
            .unwrap();
        assert_eq!(
            acc.margin,
            // - fee
            Margin::new(quote!(1010) - quote!(0.11), quote!(0), quote!(0)).unwrap()
        );
        assert_eq!(acc.position, Position::new(base!(0), quote!(100)));
    }

    #[test]
    fn mark_to_market() {
        todo!()
    }
}
