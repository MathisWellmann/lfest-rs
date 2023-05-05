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
