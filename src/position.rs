use std::{cmp::Ordering, ops::Neg};

use const_decimal::Decimal;
use num_traits::Zero;

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
        init_margin_req: Decimal<I, D>,
        fees: BaseOrQuote::PairedCurrency,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) {
        assert2::debug_assert!(
            filled_qty > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );
        match self {
            Position::Neutral => {
                debug_assert_eq!(
                    balances.position_margin,
                    BaseOrQuote::PairedCurrency::zero()
                );
                match side {
                    Side::Buy => {
                        *self = Position::Long(PositionInner::new(
                            filled_qty,
                            fill_price,
                            init_margin_req,
                            fees,
                            balances,
                        ))
                    }
                    Side::Sell => {
                        *self = Position::Short(PositionInner::new(
                            filled_qty,
                            fill_price,
                            init_margin_req,
                            fees,
                            balances,
                        ))
                    }
                }
            }
            Position::Long(inner) => match side {
                Side::Buy => {
                    inner.increase_contracts(
                        filled_qty,
                        fill_price,
                        init_margin_req,
                        fees,
                        balances,
                    );
                }
                Side::Sell => match filled_qty.cmp(&inner.quantity()) {
                    Ordering::Less => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            init_margin_req,
                            1,
                            fees,
                            balances,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            init_margin_req,
                            1,
                            fees,
                            balances,
                        );
                        *self = Position::Neutral;
                        debug_assert_eq!(
                            balances.position_margin,
                            BaseOrQuote::PairedCurrency::zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_short_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            init_margin_req,
                            1,
                            fees,
                            balances,
                        );
                        assert_eq!(inner.quantity(), BaseOrQuote::zero());
                        debug_assert_eq!(
                            balances.position_margin,
                            BaseOrQuote::PairedCurrency::zero()
                        );
                        *self = Position::Short(PositionInner::new(
                            new_short_qty,
                            fill_price,
                            init_margin_req,
                            BaseOrQuote::PairedCurrency::zero(),
                            balances,
                        ));
                    }
                },
            },
            Position::Short(inner) => match side {
                Side::Buy => match filled_qty.cmp(&inner.quantity()) {
                    Ordering::Less => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            init_margin_req,
                            -1,
                            fees,
                            balances,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            init_margin_req,
                            -1,
                            fees,
                            balances,
                        );
                        *self = Position::Neutral;
                        debug_assert_eq!(
                            balances.position_margin,
                            BaseOrQuote::PairedCurrency::zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_long_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            init_margin_req,
                            -1,
                            fees,
                            balances,
                        );
                        assert_eq!(inner.quantity(), BaseOrQuote::zero());
                        debug_assert_eq!(
                            balances.position_margin,
                            BaseOrQuote::PairedCurrency::zero()
                        );
                        *self = Position::Long(PositionInner::new(
                            new_long_qty,
                            fill_price,
                            init_margin_req,
                            BaseOrQuote::PairedCurrency::zero(),
                            balances,
                        ));
                    }
                },
                Side::Sell => {
                    inner.increase_contracts(
                        filled_qty,
                        fill_price,
                        init_margin_req,
                        fees,
                        balances,
                    );
                }
            },
        };
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

    #[test]
    #[tracing_test::traced_test]
    fn position_change_position() {
        let qty = BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap());
        let entry_price = QuoteCurrency::from(Decimal::try_from_scaled(958423665, 5).unwrap());
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin_req = Decimal::ONE;
        let fees = QuoteCurrency::zero();

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let mut pos = Position::Short(PositionInner::new(
            qty,
            entry_price,
            init_margin_req,
            fees,
            &mut balances,
        ));

        pos.change_position(
            BaseCurrency::new(317, 3),
            QuoteCurrency::new(3020427, 2),
            Side::Buy,
            init_margin_req,
            fees,
            &mut balances,
        );
    }

    #[test]
    #[tracing_test::traced_test]
    #[ignore]
    fn position_change_position_2() {
        let mut pos = Position::Long(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(16800, 5).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(5949354994, 5).unwrap()),
        ));
        let fee = QuoteCurrency::from(Decimal::try_from_scaled(600056, 5).unwrap());
        let filled_qty = BaseCurrency::new(16800, 5);
        let fill_price = QuoteCurrency::new(6001260000, 5);
        let init_margin_req = Decimal::ONE;
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        pos.change_position(
            filled_qty,
            fill_price,
            Side::Sell,
            init_margin_req,
            fee,
            &mut balances,
        );
    }

    #[test]
    fn size_of_position() {
        assert_eq!(
            std::mem::size_of::<Position<i64, 5, BaseCurrency<_, 5>>>(),
            24
        );
    }
}
