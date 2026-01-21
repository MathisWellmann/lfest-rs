use std::{
    cmp::Ordering,
    ops::Neg,
};

use Position::*;
use num_traits::Zero;
use tracing::debug;

use crate::{
    position_inner::PositionInner,
    prelude::{
        Currency,
        Mon,
        QuoteCurrency,
    },
    types::{
        Balances,
        MarginCurrency,
        Side,
    },
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
        assert2::debug_assert!(bid > Zero::zero());
        assert2::debug_assert!(ask > Zero::zero());
        match self {
            Neutral => BaseOrQuote::PairedCurrency::zero(),
            Long(inner) => inner.unrealized_pnl(bid),
            Short(inner) => inner.unrealized_pnl(ask).neg(),
        }
    }

    /// The quantity of the position, is negative when short.
    pub fn quantity(&self) -> BaseOrQuote {
        match self {
            Neutral => BaseOrQuote::zero(),
            Long(inner) => inner.quantity(),
            Short(inner) => inner.quantity().neg(),
        }
    }

    /// The entry price of the position which is the total cost of the position relative to its quantity.
    pub fn entry_price(&self) -> QuoteCurrency<I, D> {
        match self {
            Neutral => QuoteCurrency::zero(),
            Long(inner) => inner.entry_price(),
            Short(inner) => inner.entry_price(),
        }
    }

    /// The total value of the position which is composed of quantity and avg. entry price.
    pub fn notional(&self) -> BaseOrQuote::PairedCurrency {
        match self {
            Neutral => BaseOrQuote::PairedCurrency::zero(),
            Long(inner) => inner.notional(),
            Short(inner) => inner.notional(),
        }
    }

    /// Change a position while doing proper accounting and balance transfers.
    pub fn change(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) {
        use Position::*;
        use Side::*;

        debug!("Position.change {self}, {side} {filled_qty} @ {fill_price}, balances: {balances}");
        assert2::debug_assert!(
            filled_qty > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );

        // TODO: Performance can be much better I'm sure.
        use Ordering::*;
        let pnl = match self {
            Neutral => {
                match side {
                    Buy => *self = Long(PositionInner::new(filled_qty, fill_price)),
                    Sell => *self = Short(PositionInner::new(filled_qty, fill_price)),
                }
                Zero::zero()
            }
            Long(inner) => match side {
                Buy => {
                    inner.increase_contracts(filled_qty, fill_price);
                    Zero::zero()
                }
                Sell => match filled_qty.cmp(&inner.quantity()) {
                    Less => {
                        let pnl = inner.decrease_contracts(filled_qty, fill_price, true);
                        balances.apply_pnl(pnl);
                        Zero::zero()
                    }
                    Equal => {
                        let pnl = inner.decrease_contracts(filled_qty, fill_price, true);

                        *self = Neutral;
                        pnl
                    }
                    Greater => {
                        let new_short_qty = filled_qty - inner.quantity();
                        let pnl = inner.decrease_contracts(inner.quantity(), fill_price, true);
                        balances.apply_pnl(pnl);
                        debug_assert_eq!(inner.quantity(), BaseOrQuote::zero());

                        *self = Short(PositionInner::new(new_short_qty, fill_price));
                        Zero::zero()
                    }
                },
            },
            Short(inner) => match side {
                Buy => match filled_qty.cmp(&inner.quantity()) {
                    Less => inner.decrease_contracts(filled_qty, fill_price, false),
                    Equal => {
                        let pnl = inner.decrease_contracts(filled_qty, fill_price, false);

                        *self = Neutral;
                        pnl
                    }
                    Greater => {
                        let new_long_qty = filled_qty - inner.quantity();
                        let pnl = inner.decrease_contracts(inner.quantity(), fill_price, false);
                        debug_assert_eq!(inner.quantity(), BaseOrQuote::zero());

                        *self = Long(PositionInner::new(new_long_qty, fill_price));
                        pnl
                    }
                },
                Sell => {
                    inner.increase_contracts(filled_qty, fill_price);
                    Zero::zero()
                }
            },
        };
        balances.apply_pnl(pnl);
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
            Neutral => write!(f, "Neutral"),
            Long(inner) => {
                write!(f, "Long {} @ {}", inner.quantity(), inner.entry_price())
            }
            Short(inner) => {
                write!(f, "Short {} @ {}", inner.quantity(), inner.entry_price())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;
    use num::Signed;
    use proptest::prelude::*;

    use super::*;
    use crate::prelude::*;

    #[test]
    fn position_display() {
        let pos = Short(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(958423665, 5).unwrap()),
        ));
        assert_eq!(&pos.to_string(), "Short 0.31700 Base @ 9584.23665 Quote");
    }

    #[test]
    #[tracing_test::traced_test]
    fn position_change_position() {
        let qty = BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap());
        let entry_price = QuoteCurrency::from(Decimal::try_from_scaled(9584_23, 2).unwrap());

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));

        let mut pos = Short(PositionInner::new(qty, entry_price));

        let exit_price = QuoteCurrency::new(30204_27, 2);
        pos.change(qty, exit_price, Side::Buy, &mut balances);
        assert_eq!(pos, Neutral);
        assert_eq!(
            balances,
            Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0) - QuoteCurrency::new(6536_55268, 5))
                .total_fees_paid(Zero::zero())
                .build()
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn position_change_position_2() {
        let mut pos = Long(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(16800, 5).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(5949354994, 5).unwrap()),
        ));
        let filled_qty = BaseCurrency::new(16800, 5);
        let fill_price = QuoteCurrency::new(6001260000, 5);
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        pos.change(filled_qty, fill_price, Side::Sell, &mut balances);
    }

    #[test]
    fn size_of_position() {
        assert_eq!(size_of::<Position<i32, 4, BaseCurrency<_, 4>>>(), 12);
        assert_eq!(size_of::<Position<i64, 5, BaseCurrency<_, 5>>>(), 24);
    }

    proptest! {
        #[test]
        fn position_change_proptest_neutral(qty in 1..100_i64, fill_price in 1..100_i64, do_buy in 0..2_i32) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);

            let mut position = Neutral;
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            assert!(do_buy == 0 || do_buy == 1);
            let side = if do_buy == 0 {
                Side::Buy
            } else {
                Side::Sell
            };
            position.change(
                filled_qty,
                fill_price,
                side,
                &mut balances,
            );
            match side {
                Side::Buy => assert_eq!(position, Long(PositionInner::new(filled_qty, fill_price))),
                Side::Sell => assert_eq!(position, Short(PositionInner::new(filled_qty, fill_price))),
            }
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0))
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_long_sell(qty in 1..100_i64, fill_price in 1..100_i64) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Long(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Sell,
                &mut balances,
            );
            let new_qty = (start_qty - filled_qty).abs();
            if filled_qty > start_qty {
                assert_eq!(position, Short(PositionInner::new(new_qty, fill_price)));
            } else if filled_qty < start_qty {
                assert_eq!(position, Long(PositionInner::new(new_qty, fill_price)));
            } else {
                assert_eq!(position, Neutral);
            }
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0))
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_long_buy(qty in 1..50_i64, fill_price in 50..100_i64) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Long(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Buy,
                &mut balances,
            );
            let new_qty = start_qty + filled_qty;
            assert_eq!(position, Long(PositionInner::new(new_qty, fill_price)));
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0))
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_short_buy(qty in 1..100_i64, fill_price in 1..100_i64) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Short(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Buy,
                &mut balances,
            );
            let new_qty = (start_qty - filled_qty).abs();
            if filled_qty > start_qty {
                assert_eq!(position, Long(PositionInner::new(new_qty, fill_price)));
            } else if filled_qty < start_qty {
                assert_eq!(position, Short(PositionInner::new(new_qty, fill_price)));
            } else {
                assert_eq!(position, Neutral);
            }
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0))
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_short_sell(qty in 1..50_i64) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(100, 0);

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Short(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Sell,
                &mut balances,
            );
            let new_qty = start_qty + filled_qty;
            assert_eq!(position, Short(PositionInner::new(new_qty, fill_price)));
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0))
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }
}
