use fpdec::Decimal;

use crate::{
    errors::{Error, Result},
    position::Position,
    quote,
    types::{Currency, Fee, MarginCurrency, QuoteCurrency},
};

#[derive(Debug, Clone)]
/// The users account
/// Generic over:
/// S: The `Currency` representing the order quantity
pub struct Account<S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    wallet_balance: S::PairedCurrency,
    position: Position<S::PairedCurrency>,
    taker_fee: Fee,
}

impl<S> Account<S>
where
    S: Currency + Default,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new [`Account`] instance.
    pub(crate) fn new(starting_balance: S::PairedCurrency, taker_fee: Fee) -> Self {
        let position = Position::default();

        Self {
            wallet_balance: starting_balance,
            position,
            taker_fee,
        }
    }

    /// Set a new position manually, be sure that you know what you are doing
    #[inline(always)]
    pub fn set_position(&mut self, position: Position<S::PairedCurrency>) {
        self.position = position;
    }

    /// Return a reference to the accounts position.
    #[inline(always)]
    pub fn position(&self) -> &Position<S::PairedCurrency> {
        &self.position
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
    pub(crate) fn try_increase_long(&mut self, amount: S, price: QuoteCurrency) -> Result<()> {
        if amount < S::new_zero() {
            return Err(Error::InvalidAmount);
        }
        if price < quote!(0) {
            return Err(Error::InvalidPrice);
        }
        if self.position.size() < S::new_zero() {
            return Err(Error::OpenShort);
        }

        let value = amount.convert(price);
        todo!("margin?");
        self.position
            .increase_long(amount, price)
            .expect("Increasing a position here must work; qed");

        Ok(())
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
        amount: S,
        price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) -> Result<()> {
        if amount <= S::new_zero() {
            return Err(Error::InvalidAmount);
        }
        if price < quote!(0) {
            return Err(Error::InvalidPrice);
        }
        if self.position.size() <= S::new_zero() {
            return Err(Error::OpenShort);
        }

        let value = amount.convert(price);
        let pnl = self.position.decrease_long(amount, price)?;

        let fees = value * fee;
        // Fee just vanishes as there is no one to benefit from the fee.
        let net_pnl = pnl - fees;
        todo!("realize pnl or return it");
        // self.realize_pnl(net_pnl, ts_ns);

        Ok(())
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
    pub(crate) fn try_increase_short(&mut self, amount: S, price: QuoteCurrency) -> Result<()> {
        if amount < S::new_zero() {
            return Err(Error::InvalidAmount);
        }
        if price < quote!(0) {
            return Err(Error::InvalidPrice);
        }

        todo!("margin");
        self.position
            .increase_short(amount, price)
            .expect("Increasing a position here must work; qed");

        Ok(())
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
        amount: S,
        price: QuoteCurrency,
        fee: Fee,
        ts_ns: i64,
    ) -> Result<()> {
        if amount <= S::new_zero() {
            return Err(Error::InvalidAmount);
        }
        if price < quote!(0) {
            return Err(Error::InvalidPrice);
        }
        if self.position.size() >= S::new_zero() {
            return Err(Error::OpenLong);
        }

        let pnl = self.position.decrease_short(amount, price)?;
        let fees = amount.convert(price) * fee;
        // Fee just vanishes as there is no one to benefit from the fee.
        let net_pnl = pnl - fees;
        todo!("realize profit");
        // self.realize_pnl(net_pnl, ts_ns);

        Ok(())
    }

    /// Turn a long position into a short one by,
    /// 0. ensuring there is enough balance, all things considered.
    /// 1. reducing the *existing* long position.
    /// 2. entering a new short
    pub(crate) fn try_turn_around_long(&mut self, amount: S, price: QuoteCurrency) -> Result<()> {
        if amount < S::new_zero() {
            return Err(Error::NonPositive);
        }
        if price < quote!(0) {
            return Err(Error::NonPositive);
        }

        todo!()
    }

    /// Turn a short position into a long one by,
    /// 0. ensuring there is enough balance, all things considered.
    /// 1. reducing the *existing* long position.
    /// 2. entering a new short
    pub(crate) fn try_turn_around_short(&mut self, amount: S, price: QuoteCurrency) -> Result<()> {
        if amount < S::new_zero() {
            return Err(Error::NonPositive);
        }
        if price < quote!(0) {
            return Err(Error::NonPositive);
        }

        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, fee, prelude::BaseCurrency};

    /// Create a new mock account for testing.
    fn mock_account() -> Account<BaseCurrency> {
        Account::new(quote!(1000), fee!(0.001))
    }

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
