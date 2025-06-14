use std::{cmp::Ordering, ops::Neg};

use const_decimal::Decimal;
use num::One;
use num_traits::Zero;
use tracing::debug;

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
        assert2::debug_assert!(bid > Zero::zero());
        assert2::debug_assert!(ask > Zero::zero());
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
            Position::Long(inner) => inner.notional(),
            Position::Short(inner) => inner.notional(),
        }
    }

    /// Change a position while doing proper accounting and balance transfers.
    pub fn change(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        init_margin_req: Decimal<I, D>,
    ) {
        use Position::*;
        use Side::*;

        debug!("Position.change {self}, {side} {filled_qty} @ {fill_price}, balances: {balances}");
        assert2::debug_assert!(
            filled_qty > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );
        assert2::debug_assert!(init_margin_req <= Decimal::one());
        assert2::debug_assert!(init_margin_req >= Decimal::zero());
        debug_assert_eq!(
            balances.position_margin(),
            self.total_cost() * init_margin_req
        );

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
                    Ordering::Less => {
                        let pnl = inner.decrease_contracts(filled_qty, fill_price, true);
                        balances.apply_pnl(pnl);
                        Zero::zero()
                    }
                    Ordering::Equal => {
                        let pnl = inner.decrease_contracts(filled_qty, fill_price, true);

                        *self = Neutral;
                        pnl
                    }
                    Ordering::Greater => {
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
                    Ordering::Less => inner.decrease_contracts(filled_qty, fill_price, false),
                    Ordering::Equal => {
                        let pnl = inner.decrease_contracts(filled_qty, fill_price, false);

                        *self = Neutral;
                        pnl
                    }
                    Ordering::Greater => {
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
        let new_position_margin = self.total_cost() * init_margin_req;
        assert!(new_position_margin >= Zero::zero());
        match new_position_margin.cmp(&balances.position_margin()) {
            Ordering::Less => {
                let delta = balances.position_margin() - new_position_margin;
                balances.free_position_margin(delta);
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                let delta = new_position_margin - balances.position_margin();
                let success = balances.try_reserve_position_margin(delta);
                debug_assert!(success, "Can reserve position margin");
            }
        }
        debug_assert_eq!(new_position_margin, balances.position_margin());
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
    use num::Signed;
    use proptest::prelude::*;

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
        pos.change(qty, exit_price, Side::Buy, &mut balances, init_margin_req);
        assert_eq!(pos, Position::Neutral);
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
        pos.change(
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

    proptest! {
        #[test]
        fn position_change_proptest_neutral(qty in 1..100_i64, fill_price in 1..100_i64, leverage in 1..10_u8, do_buy in 0..2_i32) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);
            let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
            let notional = QuoteCurrency::convert_from(filled_qty, fill_price);
            let margin = notional * init_margin_req;

            let mut position = Position::Neutral;
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
                init_margin_req,
            );
            match side {
                Side::Buy => assert_eq!(position, Position::Long(PositionInner::new(filled_qty, fill_price))),
                Side::Sell => assert_eq!(position, Position::Short(PositionInner::new(filled_qty, fill_price))),
            }
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - margin)
                .position_margin(margin)
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_long_sell(qty in 1..100_i64, fill_price in 1..100_i64, leverage in 1..10_u8) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);
            let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Position::Long(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
            let margin = QuoteCurrency::convert_from(start_qty, fill_price) * init_margin_req;
            assert!(balances.try_reserve_position_margin(margin));

            position.change(
                filled_qty,
                fill_price,
                Side::Sell,
                &mut balances,
                init_margin_req,
            );
            let new_qty = (start_qty - filled_qty).abs();
            if filled_qty > start_qty {
                assert_eq!(position, Position::Short(PositionInner::new(new_qty, fill_price)));
            } else if filled_qty < start_qty {
                assert_eq!(position, Position::Long(PositionInner::new(new_qty, fill_price)));
            } else {
                assert_eq!(position, Position::Neutral);
            }
            let margin = QuoteCurrency::convert_from(new_qty, fill_price) * init_margin_req;
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - margin)
                .position_margin(margin)
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_long_buy(qty in 1..50_i64, fill_price in 50..100_i64, leverage in 1..10_u8) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);
            let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Position::Long(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
            let margin = QuoteCurrency::convert_from(start_qty, fill_price) * init_margin_req;
            assert!(balances.try_reserve_position_margin(margin));

            position.change(
                filled_qty,
                fill_price,
                Side::Buy,
                &mut balances,
                init_margin_req,
            );
            let new_qty = start_qty + filled_qty;
            assert_eq!(position, Position::Long(PositionInner::new(new_qty, fill_price)));
            let margin = QuoteCurrency::convert_from(new_qty, fill_price) * init_margin_req;
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - margin)
                .position_margin(margin)
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_short_buy(qty in 1..100_i64, fill_price in 1..100_i64, leverage in 1..10_u8) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);
            let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Position::Short(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
            let margin = QuoteCurrency::convert_from(start_qty, fill_price) * init_margin_req;
            assert!(balances.try_reserve_position_margin(margin));

            position.change(
                filled_qty,
                fill_price,
                Side::Buy,
                &mut balances,
                init_margin_req,
            );
            let new_qty = (start_qty - filled_qty).abs();
            if filled_qty > start_qty {
                assert_eq!(position, Position::Long(PositionInner::new(new_qty, fill_price)));
            } else if filled_qty < start_qty {
                assert_eq!(position, Position::Short(PositionInner::new(new_qty, fill_price)));
            } else {
                assert_eq!(position, Position::Neutral);
            }
            let margin = QuoteCurrency::convert_from(new_qty, fill_price) * init_margin_req;
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - margin)
                .position_margin(margin)
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }

    proptest! {
        #[test]
        fn position_change_proptest_short_sell(qty in 1..50_i64, leverage in 1..10_u8) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(100, 0);
            let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

            let start_qty = BaseCurrency::new(50, 0);
            let mut position = Position::Short(PositionInner::new(start_qty, fill_price));
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
            let margin = QuoteCurrency::convert_from(start_qty, fill_price) * init_margin_req;
            assert!(balances.try_reserve_position_margin(margin));

            position.change(
                filled_qty,
                fill_price,
                Side::Sell,
                &mut balances,
                init_margin_req,
            );
            let new_qty = start_qty + filled_qty;
            assert_eq!(position, Position::Short(PositionInner::new(new_qty, fill_price)));
            let margin = QuoteCurrency::convert_from(new_qty, fill_price) * init_margin_req;
            assert_eq!(balances, Balances::builder()
                .available(QuoteCurrency::new(10_000, 0) - margin)
                .position_margin(margin)
                .order_margin(Zero::zero())
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }
}
