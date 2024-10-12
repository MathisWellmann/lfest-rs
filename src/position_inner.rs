use std::cmp::Ordering;

use getset::{CopyGetters, Getters};
use num_traits::{Signed, Zero};
use tracing::{debug, trace};

use crate::{
    prelude::{
        CurrencyMarker, Mon, Monies, Quote, Transaction, TransactionAccounting,
        EXCHANGE_FEE_ACCOUNT, TREASURY_ACCOUNT, USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
    },
    types::MarginCurrencyMarker,
};

/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
#[derive(Debug, Clone, Default, Eq, PartialEq, Getters, CopyGetters)]
pub struct PositionInner<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    /// The number of futures contracts making up the position.
    #[getset(get_copy = "pub")]
    quantity: Monies<T, BaseOrQuote>,

    /// The total cost paid for the position (not margin though).
    #[getset(get_copy = "pub")]
    total_cost: Monies<T, BaseOrQuote::PairedCurrency>,

    /// The outstanding fees of the position that will be payed when reducing the position.
    #[getset(get_copy = "pub")]
    outstanding_fees: Monies<T, BaseOrQuote::PairedCurrency>,
}

impl<T, BaseOrQuote> PositionInner<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
{
    /// Create a new instance.
    ///
    /// # Panics:
    /// if `quantity` or `entry_price` are invalid.
    pub fn new<Acc>(
        quantity: Monies<T, BaseOrQuote>,
        entry_price: Monies<T, Quote>,
        accounting: &mut Acc,
        init_margin_req: T,
        fees: Monies<T, BaseOrQuote::PairedCurrency>,
    ) -> Self
    where
        Acc: TransactionAccounting<T, BaseOrQuote::PairedCurrency>,
    {
        trace!("new position: qty {quantity} @ {entry_price}");
        assert!(quantity > Monies::zero());
        assert!(entry_price > Monies::zero());

        let margin =
            BaseOrQuote::PairedCurrency::convert_from(quantity, entry_price) * init_margin_req;
        let transaction =
            Transaction::new(USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT, margin);
        accounting
            .create_margin_transfer(transaction)
            .expect("margin transfer for opening a new position works.");

        Self {
            quantity,
            total_cost: BaseOrQuote::PairedCurrency::convert_from(quantity, entry_price),
            outstanding_fees: fees,
        }
    }

    /// The average price at which this position was entered into.
    pub fn entry_price(&self) -> Monies<T, Quote> {
        BaseOrQuote::PairedCurrency::price_paid_for_qty(self.total_cost, self.quantity)
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    pub fn unrealized_pnl(
        &self,
        mark_to_market_price: Monies<T, Quote>,
    ) -> Monies<T, BaseOrQuote::PairedCurrency> {
        BaseOrQuote::PairedCurrency::pnl(self.entry_price(), mark_to_market_price, self.quantity)
    }

    /// Add contracts to the position.
    pub(crate) fn increase_contracts<Acc>(
        &mut self,
        qty: Monies<T, BaseOrQuote>,
        entry_price: Monies<T, Quote>,
        accounting: &mut Acc,
        init_margin_req: T,
        fees: Monies<T, BaseOrQuote::PairedCurrency>,
    ) where
        Acc: TransactionAccounting<T, BaseOrQuote::PairedCurrency>,
    {
        debug!(
            "increase_contracts: qty: {qty} @ {entry_price}; self: {:?}",
            self
        );
        assert!(qty > Monies::zero());
        assert!(entry_price > Monies::zero());

        let value = BaseOrQuote::PairedCurrency::convert_from(qty, entry_price);

        self.quantity += qty;
        self.outstanding_fees += fees;
        self.total_cost += value;

        let margin = value * init_margin_req;
        let transaction =
            Transaction::new(USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT, margin);
        accounting
            .create_margin_transfer(transaction)
            .expect("is an internal call and must work");
    }

    /// Decrease the position.
    pub(crate) fn decrease_contracts<Acc>(
        &mut self,
        qty: Monies<T, BaseOrQuote>,
        liquidation_price: Monies<T, Quote>,
        accounting: &mut Acc,
        init_margin_req: T,
        direction_multiplier: T,
        fees: Monies<T, BaseOrQuote::PairedCurrency>,
    ) where
        Acc: TransactionAccounting<T, BaseOrQuote::PairedCurrency>,
    {
        debug!(
            "decrease_contracts: qty: {qty} @ {liquidation_price}; self: {:?}",
            self
        );
        assert!(qty > Monies::zero());
        assert!(qty <= self.quantity);
        debug_assert!(direction_multiplier == T::one() || direction_multiplier == T::from(-1));

        let entry_price = self.entry_price();

        self.quantity -= qty;
        self.outstanding_fees += fees;
        self.total_cost -= BaseOrQuote::PairedCurrency::convert_from(qty, entry_price);

        debug_assert!(self.quantity >= Monies::zero());
        if self.quantity.is_zero() {
            assert_eq!(*self.total_cost.as_ref(), T::zero());
        }

        let pnl = BaseOrQuote::PairedCurrency::pnl(
            entry_price,
            liquidation_price,
            qty * direction_multiplier,
        );
        match pnl.cmp(&Monies::zero()) {
            Ordering::Greater => {
                let transaction = Transaction::new(USER_WALLET_ACCOUNT, TREASURY_ACCOUNT, pnl);
                accounting
                    .create_margin_transfer(transaction)
                    .expect("margin transfer must work");
            }
            Ordering::Less => {
                let transaction =
                    Transaction::new(TREASURY_ACCOUNT, USER_WALLET_ACCOUNT, pnl.abs());
                accounting
                    .create_margin_transfer(transaction)
                    .expect("margin transfer must work");
            }
            Ordering::Equal => {}
        }
        let margin_to_free =
            BaseOrQuote::PairedCurrency::convert_from(qty, entry_price) * init_margin_req;
        let transaction = Transaction::new(
            USER_WALLET_ACCOUNT,
            USER_POSITION_MARGIN_ACCOUNT,
            margin_to_free,
        );
        accounting
            .create_margin_transfer(transaction)
            .expect("margin transfer must work");

        let transaction = Transaction::new(
            EXCHANGE_FEE_ACCOUNT,
            USER_WALLET_ACCOUNT,
            self.outstanding_fees,
        );
        accounting
            .create_margin_transfer(transaction)
            .expect("margin transfer must work");
        self.outstanding_fees = Monies::zero();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prelude::*, TEST_FEE_MAKER};

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_new(leverage: u32) {
        let mut ta = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1) / Decimal::from(leverage);
        let qty = base!(0.5);
        let entry_price = quote!(100);
        let fees = TEST_FEE_MAKER.for_value(Quote::convert_from(qty, entry_price));
        let pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);
        assert_eq!(
            pos,
            PositionInner {
                quantity: qty,
                total_cost: quote!(50),
                outstanding_fees: fees,
            }
        );
        assert_eq!(pos.entry_price(), quote!(100));
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            quote!(50) * init_margin_req
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(1000) - quote!(50) * init_margin_req
        );
    }

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_increase_contracts(leverage: u32) {
        let mut ta = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1) / Decimal::from(leverage);
        let qty = base!(0.5);
        let entry_price = quote!(100);
        let fee_0 = TEST_FEE_MAKER.for_value(Quote::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fee_0);

        let entry_price = quote!(150);
        let fee_1 = TEST_FEE_MAKER.for_value(Quote::convert_from(qty, entry_price));
        pos.increase_contracts(qty, entry_price, &mut ta, init_margin_req, fee_1);
        assert_eq!(
            pos,
            PositionInner {
                quantity: base!(1),
                total_cost: quote!(125),
                outstanding_fees: fee_0 + fee_1
            }
        );
        assert_eq!(pos.entry_price(), quote!(125));
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            quote!(125) * init_margin_req
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(1000) - quote!(125) * init_margin_req
        );
    }

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_decrease_contracts_basic(leverage: u32) {
        let mut ta = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1) / Decimal::from(leverage);
        let qty = base!(5);
        let entry_price = quote!(100);
        let fees = TEST_FEE_MAKER.for_value(Quote::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);
        pos.decrease_contracts(
            qty / BaseCurrency::new(Dec!(2)),
            entry_price,
            &mut ta,
            init_margin_req,
            Dec!(1),
            fees / QuoteCurrency::new(Dec!(2)),
        );
        assert_eq!(
            pos,
            PositionInner {
                quantity: base!(2.5),
                total_cost: quote!(250),
                outstanding_fees: quote!(0),
            }
        );
        assert_eq!(pos.entry_price(), quote!(100));
        let margin = quote!(250) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin,
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(1000) - margin - fees * Dec!(1.5)
        );

        pos.decrease_contracts(
            qty / BaseCurrency::new(Dec!(2)),
            entry_price,
            &mut ta,
            init_margin_req,
            Dec!(1),
            fees / QuoteCurrency::new(Dec!(2)),
        );
        assert_eq!(
            pos,
            PositionInner {
                quantity: base!(0),
                total_cost: quote!(0),
                outstanding_fees: quote!(0),
            }
        );
        assert_eq!(pos.entry_price(), quote!(0));
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            quote!(0)
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(1000) - fees * Dec!(2)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_win(leverage: u32, position_side: Side) {
        let mut ta = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1) / Decimal::from(leverage);
        let qty = base!(5);
        let entry_price = quote!(100);
        let fees = TEST_FEE_MAKER.for_value(Quote::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);

        let exit_price = quote!(110);
        let side_mult = match position_side {
            Side::Buy => Dec!(1),
            Side::Sell => Dec!(-1),
        };
        pos.decrease_contracts(
            qty / BaseCurrency::new(Dec!(2)),
            exit_price,
            &mut ta,
            init_margin_req,
            side_mult,
            fees / QuoteCurrency::new(Dec!(2)),
        );

        assert_eq!(pos.quantity(), base!(2.5));
        assert_eq!(pos.entry_price(), quote!(100));
        assert_eq!(pos.total_cost(), quote!(250));
        let margin = quote!(250) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin
        );
        let profit = quote!(25) * side_mult;
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(1000) + profit - margin - fees * Dec!(1.5)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_2(leverage: u32, position_side: Side) {
        let mut ta = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1) / Decimal::from(leverage);
        let qty = base!(5);
        let entry_price = quote!(100);
        let fees = TEST_FEE_MAKER.for_value(Quote::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);

        let exit_price = quote!(90);
        let side_mult = match position_side {
            Side::Buy => Dec!(1),
            Side::Sell => Dec!(-1),
        };
        pos.decrease_contracts(
            qty / BaseCurrency::new(Dec!(2)),
            exit_price,
            &mut ta,
            init_margin_req,
            side_mult,
            fees / QuoteCurrency::new(Dec!(2)),
        );

        assert_eq!(pos.quantity(), base!(2.5));
        assert_eq!(pos.entry_price(), quote!(100));
        assert_eq!(pos.total_cost(), quote!(250));
        let margin = quote!(250) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin
        );
        let loss = quote!(25) * side_mult;
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(1000) - loss - margin - fees * Dec!(1.5)
        );
    }

    #[tracing_test::traced_test]
    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_decrease_contracts_inverse(leverage: u32) {
        let mut ta = InMemoryTransactionAccounting::new(base!(10));
        let init_margin_req = Dec!(1) / Decimal::from(leverage);
        let qty = quote!(500);
        let entry_price = quote!(100);
        let fees = TEST_FEE_MAKER.for_value(Base::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);

        let exit_price = quote!(200);
        pos.decrease_contracts(
            qty / QuoteCurrency::new(Dec!(2)),
            exit_price,
            &mut ta,
            init_margin_req,
            Dec!(1),
            fees / BaseCurrency::new(Dec!(2)),
        );

        assert_eq!(pos.quantity(), quote!(250));
        assert_eq!(pos.entry_price(), quote!(100));
        assert_eq!(pos.total_cost(), base!(2.5));
        let margin = base!(2.5) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            base!(11.25) - margin - fees * Dec!(1.5)
        );
    }

    #[test_case::test_matrix([1, 2, 5, 9])]
    fn position_inner_entry_price_linear(qty: u32) {
        let qty = BaseCurrency::from(Decimal::from(qty));
        let mut ta = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1);
        let fees = quote!(0);
        let pos = PositionInner::new(qty, quote!(100), &mut ta, init_margin_req, fees);
        assert_eq!(pos.entry_price(), quote!(100));
    }

    #[test_case::test_matrix([10, 20, 50, 90])]
    fn position_inner_entry_price_inverse(qty: u32) {
        let qty = QuoteCurrency::from(Decimal::from(qty));
        let mut ta = InMemoryTransactionAccounting::new(base!(10));
        let init_margin_req = Dec!(1);
        let fees = base!(0);
        let pos = PositionInner::new(qty, quote!(100), &mut ta, init_margin_req, fees);
        assert_eq!(pos.entry_price(), quote!(100));
    }
}
