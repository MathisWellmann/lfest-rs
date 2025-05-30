use getset::{CopyGetters, Getters};
use num_traits::Zero;
use tracing::trace;

use crate::{
    prelude::{Currency, Mon, QuoteCurrency},
    types::MarginCurrency,
};

/// Describes the position information of the account.
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

    /// Create a new position.
    ///
    /// # Panics:
    /// if `quantity` or `entry_price` are invalid.
    pub fn new(quantity: BaseOrQuote, entry_price: QuoteCurrency<I, D>) -> Self {
        trace!("Position::new: {quantity} @ {entry_price}");
        assert2::debug_assert!(quantity > BaseOrQuote::zero());
        assert2::debug_assert!(entry_price > QuoteCurrency::zero());

        Self {
            quantity,
            entry_price,
        }
    }

    /// The value of the position at its entry price.
    #[inline(always)]
    pub fn notional(&self) -> BaseOrQuote::PairedCurrency {
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
    #[inline(always)]
    pub(crate) fn increase_contracts(
        &mut self,
        qty: BaseOrQuote,
        entry_price: QuoteCurrency<I, D>,
    ) {
        trace!("increase_contracts: {qty} @ {entry_price}; self: {}", self);
        assert2::debug_assert!(qty > BaseOrQuote::zero());
        assert2::debug_assert!(entry_price > QuoteCurrency::zero());

        let new_entry_price = QuoteCurrency::new_weighted_price(
            self.entry_price,
            *self.quantity.as_ref(),
            entry_price,
            *qty.as_ref(),
        );
        assert2::debug_assert!(new_entry_price > QuoteCurrency::zero());

        self.quantity += qty;
        self.entry_price = new_entry_price;
    }

    /// Decrease the position.
    #[must_use] // Returns the pnl
    #[inline(always)]
    pub(crate) fn decrease_contracts(
        &mut self,
        qty: BaseOrQuote,
        exit_price: QuoteCurrency<I, D>,
        is_long: bool,
    ) -> BaseOrQuote::PairedCurrency {
        trace!("decrease_contracts: {qty} @ {exit_price}; self: {}", self);
        assert2::debug_assert!(qty > BaseOrQuote::zero());
        assert2::debug_assert!(qty <= self.quantity);

        let entry_price = self.entry_price();
        assert2::debug_assert!(entry_price > QuoteCurrency::zero());

        self.quantity -= qty;
        assert2::debug_assert!(self.quantity >= BaseOrQuote::zero());

        BaseOrQuote::PairedCurrency::pnl(entry_price, exit_price, if is_long { qty } else { -qty })
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;
    use num_traits::One;

    use super::*;
    use crate::{DECIMALS, prelude::*};

    #[test]
    fn position_inner_new() {
        let qty = BaseCurrency::<i64, 5>::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);

        let pos = PositionInner::new(qty, entry_price);
        assert_eq!(
            pos,
            PositionInner {
                quantity: qty,
                entry_price,
            }
        );
    }

    #[test]
    fn position_inner_increase_contracts() {
        let qty = BaseCurrency::<i64, 5>::new(5, 1);
        let entry_price = QuoteCurrency::new(100, 0);

        let mut pos = PositionInner::new(qty, entry_price);

        let entry_price = QuoteCurrency::new(150, 0);
        pos.increase_contracts(qty, entry_price);
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::one(),
                entry_price: QuoteCurrency::new(125, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(125, 0));
    }

    #[test]
    fn position_inner_decrease_contracts_basic() {
        let qty = BaseCurrency::<i64, 5>::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let mut pos = PositionInner::new(qty, entry_price);
        let pnl = pos.decrease_contracts(qty / BaseCurrency::new(2, 0), entry_price, true);
        assert_eq!(pnl, QuoteCurrency::zero());
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::new(25, 1),
                entry_price: QuoteCurrency::new(100, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));

        let pnl = pos.decrease_contracts(qty / BaseCurrency::new(2, 0), entry_price, true);
        assert_eq!(pnl, QuoteCurrency::zero());
        assert_eq!(
            pos,
            PositionInner {
                quantity: BaseCurrency::new(0, 0),
                entry_price: QuoteCurrency::new(100, 0),
            }
        );
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
    }

    #[test_case::test_matrix(
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_win(entry_side: Side) {
        let qty = BaseCurrency::<i64, 5>::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let mut pos = PositionInner::new(qty, entry_price);

        let exit_price = QuoteCurrency::new(110, 0);
        let side_mult = match entry_side {
            Side::Buy => true,
            Side::Sell => false,
        };
        let pnl = pos.decrease_contracts(qty / BaseCurrency::new(2, 0), exit_price, side_mult);
        match entry_side {
            Side::Buy => assert_eq!(pnl, QuoteCurrency::new(25, 0)),
            Side::Sell => assert_eq!(pnl, QuoteCurrency::new(-25, 0)),
        }
        assert_eq!(pos.quantity(), BaseCurrency::new(25, 1));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.notional(), QuoteCurrency::new(250, 0));
    }

    #[test_case::test_matrix(
        [Side::Buy, Side::Sell]
    )]
    fn position_inner_decrease_contracts_2(entry_side: Side) {
        let qty = BaseCurrency::<i64, 5>::new(5, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let mut pos = PositionInner::new(qty, entry_price);

        let exit_price = QuoteCurrency::new(90, 0);
        let side_mult = match entry_side {
            Side::Buy => true,
            Side::Sell => false,
        };
        let pnl = pos.decrease_contracts(qty / BaseCurrency::new(2, 0), exit_price, side_mult);
        match entry_side {
            Side::Buy => assert_eq!(pnl, QuoteCurrency::new(-25, 0)),
            Side::Sell => assert_eq!(pnl, QuoteCurrency::new(25, 0)),
        }
        assert_eq!(pos.quantity(), BaseCurrency::new(25, 1));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.notional(), QuoteCurrency::new(250, 0));
    }

    #[tracing_test::traced_test]
    #[test]
    fn position_inner_decrease_contracts_inverse() {
        let qty = QuoteCurrency::<i64, 5>::new(500, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let val = BaseCurrency::convert_from(qty, entry_price);
        assert_eq!(val, BaseCurrency::new(5, 0));
        let mut pos = PositionInner::new(qty, entry_price);

        let exit_price = QuoteCurrency::new(200, 0);
        let pnl = pos.decrease_contracts(qty / QuoteCurrency::new(2, 0), exit_price, true);
        assert_eq!(pnl, BaseCurrency::new(125, 2));
        assert_eq!(pos.quantity(), QuoteCurrency::new(250, 0));
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
        assert_eq!(pos.notional(), BaseCurrency::new(25, 1));
    }

    #[test_case::test_matrix([1, 2, 5, 9])]
    fn position_inner_entry_price_linear(qty: i32) {
        let qty = BaseCurrency::<i32, DECIMALS>::new(qty, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let pos = PositionInner::new(qty, entry_price);
        assert_eq!(pos.entry_price(), QuoteCurrency::new(100, 0));
    }

    #[test_case::test_matrix([10, 20, 50, 90])]
    fn position_inner_entry_price_inverse(qty: i32) {
        let qty = QuoteCurrency::<i32, DECIMALS>::new(qty, 0);
        let entry_price = QuoteCurrency::new(100, 0);
        let init_margin_req = Decimal::one();

        let mut balances = Balances::new(BaseCurrency::new(10, 0));
        let notional = BaseCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let pos = PositionInner::new(qty, entry_price);
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

        let pos = PositionInner::new(qty, entry_price);
        assert_eq!(&pos.to_string(), "PositionInner( 0.5 Base )");
    }
}
