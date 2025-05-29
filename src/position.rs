use std::{cmp::Ordering, ops::Neg};

use const_decimal::Decimal;
use num::One;
use num_traits::Zero;
use tracing::trace;

use crate::{
    position_inner::PositionInner,
    prelude::{Currency, Mon, QuoteCurrency},
    types::{Balances, MarginCurrency, Side},
};

/// A futures position can be one of three variants.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub enum Position<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// No position present.
    #[default]
    Neutral,
    /// A position in the long direction.
    Long(PositionInner<I, D, BaseOrQuote>),
    /// A position in the short direction.
    Short(PositionInner<I, D, BaseOrQuote>),
}

impl<I, const D: u8, BaseOrQuote> Position<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    /// Return the positions unrealized profit and loss.
    pub fn unrealized_pnl(
        &self,
        bid: QuoteCurrency<I, D>,
        ask: QuoteCurrency<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        match self {
            Position::Neutral => BaseOrQuote::PairedCurrency::zero(),
            Position::Long(inner) => inner.unrealized_pnl(bid),
            Position::Short(inner) => inner.unrealized_pnl(ask).neg(),
        }
    }

    /// The quantity of the position, is negative when short.
    pub fn quantity(&self) -> BaseOrQuote {
        match self {
            Position::Neutral => BaseOrQuote::zero(),
            Position::Long(inner) => inner.quantity(),
            Position::Short(inner) => inner.quantity().neg(),
        }
    }

    /// The entry price of the position which is the total cost of the position relative to its quantity.
    pub fn entry_price(&self) -> QuoteCurrency<I, D> {
        match self {
            Position::Neutral => QuoteCurrency::zero(),
            Position::Long(inner) => inner.entry_price(),
            Position::Short(inner) => inner.entry_price(),
        }
    }

    /// The total value of the position which is composed of quantity and avg. entry price.
    pub fn total_cost(&self) -> BaseOrQuote::PairedCurrency {
        match self {
            Position::Neutral => BaseOrQuote::PairedCurrency::zero(),
            Position::Long(inner) => inner.total_cost(),
            Position::Short(inner) => inner.total_cost(),
        }
    }

    /// Change a position while doing proper accounting and balance transfers.
    pub fn change_position(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        init_margin_req: Decimal<I, D>,
    ) {
        use Position::*;
        use Side::*;

        trace!("change_position {self}, {side} {filled_qty} @ {fill_price}, balances: {balances}");
        assert2::debug_assert!(
            filled_qty > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );
        assert2::debug_assert!(init_margin_req <= Decimal::one());
        assert2::debug_assert!(init_margin_req >= Decimal::zero());
        let position_margin = self.total_cost() * init_margin_req;
        debug_assert_eq!(balances.position_margin(), position_margin);

        match self {
            Neutral => match side {
                Buy => {
                    let margin = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price)
                        * init_margin_req;
                    debug_assert!(
                        balances.try_reserve_position_margin(margin),
                        "Can reserve position margin"
                    );
                    *self = Long(PositionInner::new(filled_qty, fill_price));
                }
                Sell => {
                    let margin = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price)
                        * init_margin_req;
                    debug_assert!(
                        balances.try_reserve_position_margin(margin),
                        "Can reserve position margin"
                    );
                    *self = Short(PositionInner::new(filled_qty, fill_price));
                }
            },
            Long(inner) => match side {
                Buy => {
                    let margin = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price)
                        * init_margin_req;
                    debug_assert!(
                        balances.try_reserve_position_margin(margin),
                        "Can reserve position margin"
                    );
                    inner.increase_contracts(filled_qty, fill_price);
                }
                Sell => match filled_qty.cmp(&inner.quantity()) {
                    Ordering::Less => {
                        let margin = BaseOrQuote::PairedCurrency::convert_from(
                            filled_qty,
                            inner.entry_price(),
                        ) * init_margin_req;
                        balances.free_position_margin(margin);

                        let pnl = inner.decrease_contracts(filled_qty, fill_price, true);
                        balances.apply_pnl(pnl);
                    }
                    Ordering::Equal => {
                        let margin = BaseOrQuote::PairedCurrency::convert_from(
                            filled_qty,
                            inner.entry_price(),
                        ) * init_margin_req;
                        balances.free_position_margin(margin);

                        let pnl = inner.decrease_contracts(filled_qty, fill_price, true);
                        balances.apply_pnl(pnl);

                        *self = Neutral;
                        debug_assert_eq!(balances.position_margin(), Zero::zero());
                    }
                    Ordering::Greater => {
                        let margin = BaseOrQuote::PairedCurrency::convert_from(
                            inner.quantity(),
                            inner.entry_price(),
                        ) * init_margin_req;
                        balances.free_position_margin(margin);

                        let new_short_qty = filled_qty - inner.quantity();
                        let pnl = inner.decrease_contracts(inner.quantity(), fill_price, true);
                        balances.apply_pnl(pnl);
                        debug_assert_eq!(inner.quantity(), BaseOrQuote::zero());
                        debug_assert_eq!(balances.position_margin(), Zero::zero());

                        *self = Short(PositionInner::new(new_short_qty, fill_price));
                        let margin =
                            BaseOrQuote::PairedCurrency::convert_from(new_short_qty, fill_price)
                                * init_margin_req;
                        debug_assert!(
                            balances.try_reserve_position_margin(margin),
                            "Can reserve position margin"
                        );
                    }
                },
            },
            Short(inner) => match side {
                Buy => match filled_qty.cmp(&inner.quantity()) {
                    Ordering::Less => {
                        let margin = BaseOrQuote::PairedCurrency::convert_from(
                            filled_qty,
                            inner.entry_price(),
                        ) * init_margin_req;
                        balances.free_position_margin(margin);

                        let pnl = inner.decrease_contracts(filled_qty, fill_price, false);
                        balances.apply_pnl(pnl);
                    }
                    Ordering::Equal => {
                        let margin = BaseOrQuote::PairedCurrency::convert_from(
                            filled_qty,
                            inner.entry_price(),
                        ) * init_margin_req;
                        balances.free_position_margin(margin);

                        let pnl = inner.decrease_contracts(filled_qty, fill_price, false);
                        balances.apply_pnl(pnl);

                        *self = Neutral;
                        debug_assert_eq!(balances.position_margin(), Zero::zero());
                    }
                    Ordering::Greater => {
                        let margin = BaseOrQuote::PairedCurrency::convert_from(
                            inner.quantity(),
                            inner.entry_price(),
                        ) * init_margin_req;
                        balances.free_position_margin(margin);

                        let new_long_qty = filled_qty - inner.quantity();
                        let pnl = inner.decrease_contracts(inner.quantity(), fill_price, false);
                        balances.apply_pnl(pnl);
                        debug_assert_eq!(inner.quantity(), BaseOrQuote::zero());
                        debug_assert_eq!(balances.position_margin(), Zero::zero());

                        *self = Long(PositionInner::new(new_long_qty, fill_price));
                        let margin =
                            BaseOrQuote::PairedCurrency::convert_from(new_long_qty, fill_price)
                                * init_margin_req;
                        debug_assert!(
                            balances.try_reserve_position_margin(margin),
                            "Can reserve position margin"
                        );
                    }
                },
                Sell => {
                    inner.increase_contracts(filled_qty, fill_price);
                    let margin = BaseOrQuote::PairedCurrency::convert_from(filled_qty, fill_price)
                        * init_margin_req;
                    debug_assert!(
                        balances.try_reserve_position_margin(margin),
                        "Can reserve position margin"
                    );
                }
            },
        };
        let position_margin = self.total_cost() * init_margin_req;
        debug_assert_eq!(balances.position_margin(), position_margin);
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for Position<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Position::Neutral => write!(f, "Neutral"),
            Position::Long(inner) => {
                write!(f, "Long {} @ {}", inner.quantity(), inner.entry_price())
            }
            Position::Short(inner) => {
                write!(f, "Short {} @ {}", inner.quantity(), inner.entry_price())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;

    use super::*;
    use crate::prelude::*;

    #[test]
    fn position_display() {
        let pos = Position::Short(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(958423665, 5).unwrap()),
        ));
        assert_eq!(&pos.to_string(), "Short 0.31700 Base @ 9584.23665 Quote");
    }

    #[tracing_test::traced_test]
    #[test_case::test_matrix([1, 2, 3, 5, 10])]
    fn position_change_position(leverage: u8) {
        let qty = BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap());
        let entry_price = QuoteCurrency::from(Decimal::try_from_scaled(9584_23, 2).unwrap());

        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_position_margin(init_margin));

        let mut pos = Position::Short(PositionInner::new(qty, entry_price));

        let exit_price = QuoteCurrency::new(30204_27, 2);
        pos.change_position(qty, exit_price, Side::Buy, &mut balances, init_margin_req);
        assert_eq!(
            balances,
            Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - QuoteCurrency::new(6536_55268, 5))
                .position_margin(Zero::zero())
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
        );
    }

    #[test_case::test_matrix([1, 2, 5, 10])]
    #[tracing_test::traced_test]
    #[ignore]
    fn position_change_position_2(leverage: u8) {
        let mut pos = Position::Long(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(16800, 5).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(5949354994, 5).unwrap()),
        ));
        let filled_qty = BaseCurrency::new(16800, 5);
        let fill_price = QuoteCurrency::new(6001260000, 5);
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        pos.change_position(
            filled_qty,
            fill_price,
            Side::Sell,
            &mut balances,
            init_margin_req,
        );
    }

    #[test]
    fn size_of_position() {
        assert_eq!(
            std::mem::size_of::<Position<i64, 5, BaseCurrency<_, 5>>>(),
            24
        );
        assert_eq!(
            std::mem::size_of::<Position<i32, 4, BaseCurrency<_, 4>>>(),
            12
        );
    }
}
