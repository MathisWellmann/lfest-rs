use std::cmp::Ordering;

use const_decimal::Decimal;
use getset::{CopyGetters, Getters};
use num_traits::{Signed, Zero};
use tracing::{debug, trace};

use crate::{
    prelude::{
        CurrencyMarker, Mon, QuoteCurrency, Transaction, TransactionAccounting, TREASURY_ACCOUNT,
        USER_WALLET_ACCOUNT,
    },
    types::MarginCurrencyMarker,
};

/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
#[derive(Debug, Clone, Default, Eq, PartialEq, Getters, CopyGetters)]
pub struct PositionInner<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker<I, D>,
{
    /// The number of futures contracts making up the position.
    #[getset(get_copy = "pub")]
    quantity: BaseOrQuote,

    /// The total cost paid for the position (not margin though).
    #[getset(get_copy = "pub")]
    total_cost: BaseOrQuote::PairedCurrency,

    /// The outstanding fees of the position that will be payed when reducing the position.
    #[getset(get_copy = "pub")]
    outstanding_fees: BaseOrQuote::PairedCurrency,
}

impl<I, const D: u8, BaseOrQuote> PositionInner<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<I, D>,
{
    /// Create a new instance.
    ///
    /// # Panics:
    /// if `quantity` or `entry_price` are invalid.
    pub fn new<Acc>(
        quantity: BaseOrQuote,
        entry_price: QuoteCurrency<I, D>,
        accounting: &mut Acc,
        init_margin_req: Decimal<I, D>,
        fees: BaseOrQuote::PairedCurrency,
    ) -> Self
    where
        Acc: TransactionAccounting<I, D, BaseOrQuote::PairedCurrency>,
    {
        trace!("new position: qty {quantity} @ {entry_price}");
        assert!(quantity > BaseOrQuote::zero());
        assert!(entry_price > QuoteCurrency::zero());

        // let margin =
        //     BaseOrQuote::PairedCurrency::convert_from(quantity, entry_price) * init_margin_req;
        // let transaction =
        //     Transaction::new(USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT, margin);
        // accounting
        //     .create_margin_transfer(transaction)
        //     .expect("margin transfer for opening a new position works.");

        // Self {
        //     quantity,
        //     total_cost: BaseOrQuote::PairedCurrency::convert_from(quantity, entry_price),
        //     outstanding_fees: fees,
        // }
        todo!()
    }

    /// The average price at which this position was entered into.
    pub fn entry_price(&self) -> QuoteCurrency<I, D> {
        BaseOrQuote::PairedCurrency::price_paid_for_qty(self.total_cost, self.quantity)
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    pub fn unrealized_pnl(
        &self,
        mark_to_market_price: QuoteCurrency<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        BaseOrQuote::PairedCurrency::pnl(self.entry_price(), mark_to_market_price, self.quantity)
    }

    /// Add contracts to the position.
    pub(crate) fn increase_contracts<Acc>(
        &mut self,
        qty: BaseOrQuote,
        entry_price: QuoteCurrency<I, D>,
        accounting: &mut Acc,
        init_margin_req: Decimal<I, D>,
        fees: BaseOrQuote::PairedCurrency,
    ) where
        Acc: TransactionAccounting<I, D, BaseOrQuote::PairedCurrency>,
    {
        debug!(
            "increase_contracts: qty: {qty} @ {entry_price}; self: {:?}",
            self
        );
        assert!(qty > BaseOrQuote::zero());
        assert!(entry_price > QuoteCurrency::zero());

        let value = BaseOrQuote::PairedCurrency::convert_from(qty, entry_price);

        self.quantity += qty;
        self.outstanding_fees += fees;
        self.total_cost += value;

        // let margin = value * init_margin_req;
        // let transaction =
        //     Transaction::new(USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT, margin);
        // accounting
        //     .create_margin_transfer(transaction)
        //     .expect("is an internal call and must work");
        todo!()
    }

    /// Decrease the position.
    pub(crate) fn decrease_contracts<Acc>(
        &mut self,
        qty: BaseOrQuote,
        liquidation_price: QuoteCurrency<I, D>,
        accounting: &mut Acc,
        init_margin_req: Decimal<I, D>,
        direction_multiplier: i8,
        fees: BaseOrQuote::PairedCurrency,
    ) where
        Acc: TransactionAccounting<I, D, BaseOrQuote::PairedCurrency>,
    {
        debug!(
            "decrease_contracts: qty: {qty} @ {liquidation_price}; self: {:?}",
            self
        );
        assert!(qty > BaseOrQuote::zero());
        assert!(qty <= self.quantity);
        debug_assert!(direction_multiplier == 1 || direction_multiplier == -1);

        let entry_price = self.entry_price();

        self.quantity -= qty;
        self.outstanding_fees += fees;
        self.total_cost -= BaseOrQuote::PairedCurrency::convert_from(qty, entry_price);

        debug_assert!(self.quantity >= BaseOrQuote::zero());
        if self.quantity.is_zero() {
            assert_eq!(self.total_cost, BaseOrQuote::PairedCurrency::zero());
        }

        let pnl = BaseOrQuote::PairedCurrency::pnl(
            entry_price,
            liquidation_price,
            if direction_multiplier == 1 { qty } else { -qty },
        );
        match pnl.cmp(&BaseOrQuote::PairedCurrency::zero()) {
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
        // let margin_to_free =
        //     BaseOrQuote::PairedCurrency::convert_from(qty, entry_price) * init_margin_req;
        // let transaction = Transaction::new(
        //     USER_WALLET_ACCOUNT,
        //     USER_POSITION_MARGIN_ACCOUNT,
        //     margin_to_free,
        // );
        // accounting
        //     .create_margin_transfer(transaction)
        //     .expect("margin transfer must work");

        // let transaction = Transaction::new(
        //     EXCHANGE_FEE_ACCOUNT,
        //     USER_WALLET_ACCOUNT,
        //     self.outstanding_fees,
        // );
        // accounting
        //     .create_margin_transfer(transaction)
        //     .expect("margin transfer must work");
        // self.outstanding_fees = BaseOrQuote::PairedCurrency::zero();
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;
    use num_traits::One;

    use super::*;
    use crate::{prelude::*, TEST_FEE_MAKER};

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_new(leverage: u8) {
        let mut ta = InMemoryTransactionAccounting::new(QuoteCurrency::<i32, 4, 2>::new(1000, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);
        let fees = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);
        assert_eq!(
            pos,
            PositionInner {
                quantity: qty,
                total_cost: QuoteCurrency::new(50, 0),
                outstanding_fees: fees,
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            QuoteCurrency::new(50, 0) * init_margin_req
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            QuoteCurrency::new(1000, 0) - QuoteCurrency::new(50, 0) * init_margin_req
        );
    }

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_increase_contracts(leverage: u8) {
        let mut ta = InMemoryTransactionAccounting::new(QuoteCurrency::<i32, 4, 2>::new(1000, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);
        let fee_0 = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fee_0);

        let entry_price = QuoteCurrency::new(150, 0);
        let fee_1 = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        pos.increase_contracts(qty, entry_price, &mut ta, init_margin_req, fee_1);
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::one(),
                total_cost: QuoteCurrency::new(125, 0),
                outstanding_fees: fee_0 + fee_1
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(125, 0));
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            QuoteCurrency::new(125, 0) * init_margin_req
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            QuoteCurrency::new(1000, 0) - QuoteCurrency::new(125, 0) * init_margin_req
        );
    }

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_decrease_contracts_basic(leverage: u8) {
        let mut ta = InMemoryTransactionAccounting::new(QuoteCurrency::<i32, 4, 2>::new(1000, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let fees = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);
        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            entry_price,
            &mut ta,
            init_margin_req,
            1,
            fees / QuoteCurrency::new(2, 0),
        );
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::new(25, 1),
                total_cost: QuoteCurrency::new(250, 0),
                outstanding_fees: QuoteCurrency::new(0, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        let margin = QuoteCurrency::new(250, 0) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin,
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            QuoteCurrency::new(1000, 0) - margin - fees * Decimal::try_from_scaled(15, 1).unwrap()
        );

        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            entry_price,
            &mut ta,
            init_margin_req,
            1,
            fees / QuoteCurrency::new(2, 0),
        );
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::new(0, 0),
                total_cost: QuoteCurrency::new(0, 0),
                outstanding_fees: QuoteCurrency::new(0, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(0, 0));
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            QuoteCurrency::new(0, 0)
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            QuoteCurrency::new(1000, 0) - fees * Decimal::try_from_scaled(2, 0).unwrap()
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_win(leverage: u8, position_side: Side) {
        let mut ta = InMemoryTransactionAccounting::new(QuoteCurrency::<i32, 4, 2>::new(1000, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let fees = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);

        let exit_price = QuoteCurrency::new(110, 0);
        let side_mult = match position_side {
            Side::Buy => 1,
            Side::Sell => -1,
        };
        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            exit_price,
            &mut ta,
            init_margin_req,
            side_mult,
            fees / QuoteCurrency::new(2, 0),
        );

        assert_eq!(pos.quantity(), BaseCurrency::new(25, 1));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.total_cost(), QuoteCurrency::new(250, 0));
        let margin = QuoteCurrency::new(250, 0) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin
        );
        let profit = QuoteCurrency::new(25_i32 * side_mult as i32, 0);
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            QuoteCurrency::new(1000, 0) + profit
                - margin
                - fees * Decimal::try_from_scaled(15, 1).unwrap()
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_2(leverage: u8, position_side: Side) {
        let mut ta = InMemoryTransactionAccounting::new(QuoteCurrency::<i32, 4, 2>::new(1000, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let fees = TEST_FEE_MAKER.for_value(QuoteCurrency::convert_from(qty, entry_price));
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);

        let exit_price = QuoteCurrency::new(90, 0);
        let side_mult = match position_side {
            Side::Buy => 1,
            Side::Sell => -1,
        };
        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            exit_price,
            &mut ta,
            init_margin_req,
            side_mult,
            fees / QuoteCurrency::new(2, 0),
        );

        assert_eq!(pos.quantity(), BaseCurrency::new(25, 1));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.total_cost(), QuoteCurrency::new(250, 0));
        let margin = QuoteCurrency::new(250, 0) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin
        );
        let loss = QuoteCurrency::new(25_i32 * side_mult as i32, 0);
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            QuoteCurrency::new(1000, 0)
                - loss
                - margin
                - fees * Decimal::try_from_scaled(15, 1).unwrap()
        );
    }

    #[tracing_test::traced_test]
    #[test_case::test_matrix([1, 2, 5])]
    #[ignore]
    fn position_inner_decrease_contracts_inverse(leverage: u8) {
        let mut ta = InMemoryTransactionAccounting::new(BaseCurrency::<i32, 4, 2>::new(10, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = QuoteCurrency::new(500, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let val = BaseCurrency::convert_from(qty, entry_price);
        assert_eq!(val, BaseCurrency::new(5, 0));
        let fees = TEST_FEE_MAKER.for_value(val);
        let mut pos = PositionInner::new(qty, entry_price, &mut ta, init_margin_req, fees);

        let exit_price = QuoteCurrency::new(200, 0);
        pos.decrease_contracts(
            qty / QuoteCurrency::new(2, 0),
            exit_price,
            &mut ta,
            init_margin_req,
            1,
            fees / BaseCurrency::new(2, 0),
        );

        assert_eq!(pos.quantity(), QuoteCurrency::new(250, 0));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.total_cost(), BaseCurrency::new(25, 1));
        let margin = BaseCurrency::new(25, 1) * init_margin_req;
        assert_eq!(
            ta.margin_balance_of(USER_POSITION_MARGIN_ACCOUNT).unwrap(),
            margin
        );
        assert_eq!(
            ta.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            BaseCurrency::new(1125, 2) - margin - fees * Decimal::try_from_scaled(15, 1).unwrap()
        );
    }

    #[test_case::test_matrix([1, 2, 5, 9])]
    fn position_inner_entry_price_linear(qty: i32) {
        let qty = BaseCurrency::<i32, 4, 2>::new(qty, 0);
        let mut ta = InMemoryTransactionAccounting::new(QuoteCurrency::new(1000, 0));
        let init_margin_req = BasisPointFrac::one();
        let fees = QuoteCurrency::new(0, 0);
        let pos = PositionInner::new(
            qty,
            QuoteCurrency::new(100, 0),
            &mut ta,
            init_margin_req,
            fees,
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
    }

    #[test_case::test_matrix([10, 20, 50, 90])]
    fn position_inner_entry_price_inverse(qty: i32) {
        let qty = QuoteCurrency::<i32, 4, 2>::new(qty, 0);
        let mut ta = InMemoryTransactionAccounting::new(BaseCurrency::new(10, 0));
        let init_margin_req = BasisPointFrac::one();
        let fees = BaseCurrency::new(0, 0);
        let pos = PositionInner::new(
            qty,
            QuoteCurrency::new(100, 0),
            &mut ta,
            init_margin_req,
            fees,
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
    }
}
