use const_decimal::Decimal;
use getset::{CopyGetters, Getters};
use num::One;
use num_traits::Zero;
use tracing::{debug, trace};

use crate::{
    prelude::{Currency, Mon, QuoteCurrency},
    types::{Balances, MarginCurrency},
};

/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
#[derive(Debug, Clone, Default, Eq, PartialEq, Getters, CopyGetters)]
pub struct PositionInner<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// The number of futures contracts making up the position.
    #[getset(get_copy = "pub")]
    quantity: BaseOrQuote,

    /// The average price at which this position was entered at.
    #[getset(get_copy = "pub")]
    entry_price: QuoteCurrency<I, D>,
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for PositionInner<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PositionInner( {} )", self.quantity,)
    }
}

impl<I, const D: u8, BaseOrQuote> PositionInner<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    #[cfg(test)]
    pub(crate) fn from_parts(quantity: BaseOrQuote, entry_price: QuoteCurrency<I, D>) -> Self {
        Self {
            quantity,
            entry_price,
        }
    }

    // TODO: remove `balances` argument.
    /// Create a new position.
    ///
    /// # Panics:
    /// if `quantity` or `entry_price` are invalid.
    pub fn new(
        quantity: BaseOrQuote,
        entry_price: QuoteCurrency<I, D>,
        init_margin_req: Decimal<I, D>,
        fee: BaseOrQuote::PairedCurrency,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Self {
        trace!("new position: qty {quantity} @ {entry_price}");
        assert2::debug_assert!(quantity > BaseOrQuote::zero());
        assert2::debug_assert!(entry_price > QuoteCurrency::zero());
        assert2::debug_assert!(init_margin_req > Decimal::zero());
        assert2::debug_assert!(init_margin_req <= Decimal::one());

        // TODO: single function which computes this across the codebase.
        let margin =
            BaseOrQuote::PairedCurrency::convert_from(quantity, entry_price) * init_margin_req;
        trace!("Position::new: margin: {margin}");
        balances.fill_order(margin);
        balances.account_for_fee(fee);

        Self {
            quantity,
            entry_price,
        }
    }

    /// The cost of the position.
    #[inline(always)]
    pub fn total_cost(&self) -> BaseOrQuote::PairedCurrency {
        BaseOrQuote::PairedCurrency::convert_from(self.quantity, self.entry_price)
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    #[inline(always)]
    pub fn unrealized_pnl(
        &self,
        mark_to_market_price: QuoteCurrency<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        BaseOrQuote::PairedCurrency::pnl(self.entry_price(), mark_to_market_price, self.quantity)
    }

    /// Add contracts to the position.
    pub(crate) fn increase_contracts(
        &mut self,
        qty: BaseOrQuote,
        entry_price: QuoteCurrency<I, D>,
        init_margin_req: Decimal<I, D>,
        fee: BaseOrQuote::PairedCurrency,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) {
        debug!(
            "increase_contracts: qty: {qty} @ {entry_price}; self: {}",
            self
        );
        assert2::debug_assert!(qty > BaseOrQuote::zero());
        assert2::debug_assert!(entry_price > QuoteCurrency::zero());

        let value = BaseOrQuote::PairedCurrency::convert_from(qty, entry_price);
        let new_entry_price = QuoteCurrency::new_weighted_price(
            self.entry_price,
            *self.quantity.as_ref(),
            entry_price,
            *qty.as_ref(),
        );

        self.quantity += qty;
        self.entry_price = new_entry_price;

        let margin = value * init_margin_req;
        balances.fill_order(margin);
        balances.account_for_fee(fee);
    }

    /// Decrease the position.
    pub(crate) fn decrease_contracts(
        &mut self,
        qty: BaseOrQuote,
        liquidation_price: QuoteCurrency<I, D>,
        init_margin_req: Decimal<I, D>,
        direction_multiplier: i8,
        fee: BaseOrQuote::PairedCurrency,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) {
        trace!(
            "decrease_contracts: qty: {qty} @ {liquidation_price}; self: {}",
            self
        );
        assert2::debug_assert!(qty > BaseOrQuote::zero());
        assert2::debug_assert!(qty <= self.quantity);
        assert2::debug_assert!(direction_multiplier == 1 || direction_multiplier == -1);

        let entry_price = self.entry_price();

        self.quantity -= qty;

        assert2::debug_assert!(self.quantity >= BaseOrQuote::zero());

        let pnl = BaseOrQuote::PairedCurrency::pnl(
            entry_price,
            liquidation_price,
            if direction_multiplier == 1 { qty } else { -qty },
        );
        balances.apply_pnl(pnl);

        let margin_to_free =
            BaseOrQuote::PairedCurrency::convert_from(qty, entry_price) * init_margin_req;
        assert2::debug_assert!(margin_to_free > BaseOrQuote::PairedCurrency::zero());
        balances.free_position_margin(margin_to_free);
        balances.account_for_fee(fee);
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;
    use num_traits::One;

    use super::*;
    use crate::{DECIMALS, prelude::*, test_fee_maker};

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_new(leverage: u8) {
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let fee = notional * *test_fee_maker().as_ref();

        let init_margin = notional * init_margin_req;
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        assert!(balances.try_reserve_order_margin(init_margin));

        let pos = PositionInner::new(qty, entry_price, init_margin_req, fee, &mut balances);
        assert_eq!(
            pos,
            PositionInner {
                quantity: qty,
                entry_price,
            }
        );
        assert_eq!(
            balances.position_margin,
            QuoteCurrency::new(50, 0) * init_margin_req
        );
        assert_eq!(
            balances.available,
            QuoteCurrency::new(1000, 0) - QuoteCurrency::new(50, 0) * init_margin_req - fee
        );
    }

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_increase_contracts(leverage: u8) {
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let fee_0 = notional * *test_fee_maker().as_ref();

        let init_margin = notional * init_margin_req;
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        assert!(balances.try_reserve_order_margin(init_margin));

        let mut pos = PositionInner::new(qty, entry_price, init_margin_req, fee_0, &mut balances);

        let entry_price = QuoteCurrency::new(150, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));
        let fee_1 = notional * *test_fee_maker().as_ref();
        pos.increase_contracts(qty, entry_price, init_margin_req, fee_1, &mut balances);
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::one(),
                entry_price: QuoteCurrency::new(125, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(125, 0));
        assert_eq!(
            balances.position_margin,
            QuoteCurrency::new(125, 0) * init_margin_req
        );
        assert_eq!(
            balances.available,
            QuoteCurrency::new(1000, 0)
                - QuoteCurrency::new(125, 0) * init_margin_req
                - fee_0
                - fee_1
        );
    }

    #[test_case::test_matrix([1, 2, 5])]
    fn position_inner_decrease_contracts_basic(leverage: u8) {
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let fees = notional * *test_fee_maker().as_ref();

        let init_margin = notional * init_margin_req;
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        assert!(balances.try_reserve_order_margin(init_margin));

        let mut pos = PositionInner::new(qty, entry_price, init_margin_req, fees, &mut balances);
        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            entry_price,
            init_margin_req,
            1,
            fees / QuoteCurrency::new(2, 0),
            &mut balances,
        );
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::new(25, 1),
                entry_price: QuoteCurrency::new(100, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        let margin = QuoteCurrency::new(250, 0) * init_margin_req;
        assert_eq!(balances.position_margin, margin,);
        assert_eq!(
            balances.available,
            QuoteCurrency::new(1000, 0) - margin - fees * Decimal::try_from_scaled(15, 1).unwrap()
        );

        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            entry_price,
            init_margin_req,
            1,
            fees / QuoteCurrency::new(2, 0),
            &mut balances,
        );
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::new(0, 0),
                entry_price: QuoteCurrency::new(100, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(balances.position_margin, QuoteCurrency::new(0, 0));
        assert_eq!(balances.order_margin, QuoteCurrency::new(0, 0));
        assert_eq!(
            balances.available,
            QuoteCurrency::new(1000, 0) - fees * Decimal::try_from_scaled(2, 0).unwrap()
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_win(leverage: u8, position_side: Side) {
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let fees = notional * *test_fee_maker().as_ref();

        let init_margin = notional * init_margin_req;
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        assert!(balances.try_reserve_order_margin(init_margin));

        let mut pos = PositionInner::new(qty, entry_price, init_margin_req, fees, &mut balances);

        let exit_price = QuoteCurrency::new(110, 0);
        let side_mult = match position_side {
            Side::Buy => 1,
            Side::Sell => -1,
        };
        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            exit_price,
            init_margin_req,
            side_mult,
            fees / QuoteCurrency::new(2, 0),
            &mut balances,
        );

        assert_eq!(pos.quantity(), BaseCurrency::new(25, 1));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.total_cost(), QuoteCurrency::new(250, 0));
        let margin = QuoteCurrency::new(250, 0) * init_margin_req;
        assert_eq!(balances.position_margin, margin);
        assert_eq!(balances.order_margin, QuoteCurrency::zero());
        let profit = QuoteCurrency::new(25 * side_mult as i64, 0);
        assert_eq!(
            balances.available,
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
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let fees = notional * *test_fee_maker().as_ref();

        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let mut pos = PositionInner::new(qty, entry_price, init_margin_req, fees, &mut balances);

        let exit_price = QuoteCurrency::new(90, 0);
        let side_mult = match position_side {
            Side::Buy => 1,
            Side::Sell => -1,
        };
        pos.decrease_contracts(
            qty / BaseCurrency::new(2, 0),
            exit_price,
            init_margin_req,
            side_mult,
            fees / QuoteCurrency::new(2, 0),
            &mut balances,
        );

        assert_eq!(pos.quantity(), BaseCurrency::new(25, 1));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.total_cost(), QuoteCurrency::new(250, 0));
        let margin = QuoteCurrency::new(250, 0) * init_margin_req;
        assert_eq!(balances.position_margin, margin);
        assert_eq!(balances.order_margin, QuoteCurrency::zero());
        let loss = QuoteCurrency::new(25 * side_mult as i64, 0);
        assert_eq!(
            balances.available,
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
        let mut balances = Balances::new(BaseCurrency::new(10, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = QuoteCurrency::new(500, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let val = BaseCurrency::convert_from(qty, entry_price);
        assert_eq!(val, BaseCurrency::new(5, 0));
        let fees = val * *test_fee_maker().as_ref();
        let mut pos = PositionInner::new(qty, entry_price, init_margin_req, fees, &mut balances);

        let exit_price = QuoteCurrency::new(200, 0);
        pos.decrease_contracts(
            qty / QuoteCurrency::new(2, 0),
            exit_price,
            init_margin_req,
            1,
            fees / BaseCurrency::new(2, 0),
            &mut balances,
        );

        assert_eq!(pos.quantity(), QuoteCurrency::new(250, 0));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.total_cost(), BaseCurrency::new(25, 1));
        let margin = BaseCurrency::new(25, 1) * init_margin_req;
        assert_eq!(balances.position_margin, margin);
        assert_eq!(
            balances.available,
            BaseCurrency::new(1125, 2) - margin - fees * Decimal::try_from_scaled(15, 1).unwrap()
        );
    }

    #[test_case::test_matrix([1, 2, 5, 9])]
    fn position_inner_entry_price_linear(qty: i32) {
        let qty = BaseCurrency::<i32, DECIMALS>::new(qty, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let init_margin_req = Decimal::one();
        let fees = QuoteCurrency::new(0, 0);

        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let pos = PositionInner::new(qty, entry_price, init_margin_req, fees, &mut balances);
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
    }

    #[test_case::test_matrix([10, 20, 50, 90])]
    fn position_inner_entry_price_inverse(qty: i32) {
        let qty = QuoteCurrency::<i32, DECIMALS>::new(qty, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let init_margin_req = Decimal::one();
        let fees = BaseCurrency::new(0, 0);

        let mut balances = Balances::new(BaseCurrency::new(10, 0));
        let notional = BaseCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let pos = PositionInner::new(qty, entry_price, init_margin_req, fees, &mut balances);
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
    }

    #[test]
    fn position_inner_display() {
        let qty = BaseCurrency::<i64, 1>::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);
        let init_margin_req = Decimal::try_from_scaled(1, 0).unwrap();

        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let pos = PositionInner::new(
            qty,
            entry_price,
            init_margin_req,
            QuoteCurrency::new(1, 1),
            &mut balances,
        );
        assert_eq!(&pos.to_string(), "PositionInner( 0.5 Base )");
    }
}
