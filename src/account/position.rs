use getset::{
    CopyGetters,
    Getters,
};
use num_traits::Zero;

use super::Balances;
use crate::{
    prelude::{
        Currency,
        Mon,
        QuoteCurrency,
    },
    types::{
        MarginCurrency,
        Side,
    },
};

/// The side of the position depends on its quantity.
#[derive(Debug, Clone, Copy)]
pub enum PositionSide {
    Short,
    Neutral,
    Long,
}

/// A futures position can be one of three variants.
#[derive(Debug, Clone, Default, Eq, PartialEq, Getters, CopyGetters)]
pub struct Position<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// The number of futures contracts making up the position.
    /// Can be negative if short.
    #[getset(get_copy = "pub")]
    quantity: BaseOrQuote,

    /// The average price at which this position was entered at.
    #[getset(get_copy = "pub")]
    entry_price: QuoteCurrency<I, D>,
}

impl<I, const D: u8, BaseOrQuote> Position<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    /// Create a new Position instance.
    /// The `entry_price` must be positive.
    #[inline]
    #[must_use]
    pub fn new(quantity: BaseOrQuote, entry_price: QuoteCurrency<I, D>) -> Option<Self> {
        if entry_price <= Zero::zero() {
            return None;
        }
        Some(Self {
            quantity,
            entry_price,
        })
    }

    /// Get which side the position has, either Long, Short or Neutral.
    #[inline]
    pub fn side(&self) -> PositionSide {
        use std::cmp::Ordering::*;

        use PositionSide::*;
        match self.quantity.cmp(&Zero::zero()) {
            Less => Short,
            Equal => Neutral,
            Greater => Long,
        }
    }

    /// Return the positions unrealized profit and loss.
    #[must_use]
    #[inline(always)]
    pub fn unrealized_pnl(
        &self,
        bid: QuoteCurrency<I, D>,
        ask: QuoteCurrency<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(bid > Zero::zero());
        assert2::debug_assert!(ask > Zero::zero());

        use std::cmp::Ordering::*;
        match self.quantity.cmp(&Zero::zero()) {
            Less => BaseOrQuote::PairedCurrency::pnl(self.entry_price(), ask, self.quantity),
            Equal => Zero::zero(),
            Greater => BaseOrQuote::PairedCurrency::pnl(self.entry_price(), bid, self.quantity),
        }
    }

    /// The total value of the position which is composed of quantity and avg. entry price.
    #[must_use]
    #[inline]
    pub fn notional(&self) -> BaseOrQuote::PairedCurrency {
        if self.entry_price.is_zero() {
            Zero::zero()
        } else {
            BaseOrQuote::PairedCurrency::convert_from(self.quantity.abs(), self.entry_price)
        }
    }

    /// Change a position while doing proper accounting and balance transfers.
    #[inline]
    pub fn change(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
    ) {
        use Side::*;

        tracing::trace!(
            "Position.change {self}, {side} {filled_qty} @ {fill_price}, balances: {balances}"
        );
        assert2::debug_assert!(
            filled_qty > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );

        // TODO: simplify.
        use std::cmp::Ordering::*;
        match self.quantity.cmp(&Zero::zero()) {
            Equal => match side {
                Buy => {
                    self.entry_price = fill_price;
                    self.quantity = filled_qty;
                }
                Sell => {
                    self.entry_price = fill_price;
                    self.quantity = -filled_qty;
                }
            },
            // Long
            Greater => match side {
                Buy => {
                    self.entry_price = QuoteCurrency::new_weighted_price(
                        self.entry_price,
                        *self.quantity.as_ref(),
                        fill_price,
                        *filled_qty.as_ref(),
                    );
                    self.quantity += filled_qty;
                }
                Sell => match filled_qty.cmp(&self.quantity().abs()) {
                    Less => {
                        self.quantity -= filled_qty;
                        assert2::debug_assert!(self.quantity > Zero::zero());
                        balances.apply_pnl(BaseOrQuote::PairedCurrency::pnl(
                            self.entry_price,
                            fill_price,
                            filled_qty,
                        ));
                    }
                    Equal => {
                        balances.apply_pnl(BaseOrQuote::PairedCurrency::pnl(
                            self.entry_price,
                            fill_price,
                            filled_qty,
                        ));
                        self.quantity -= filled_qty;
                        debug_assert_eq!(self.quantity, Zero::zero());
                        self.entry_price = Zero::zero();
                    }
                    Greater => {
                        balances.apply_pnl(BaseOrQuote::PairedCurrency::pnl(
                            self.entry_price,
                            fill_price,
                            self.quantity,
                        ));
                        self.quantity -= filled_qty;
                        assert2::debug_assert!(self.quantity < Zero::zero());
                        self.entry_price = fill_price;
                    }
                },
            },
            // Short
            Less => match side {
                Buy => match filled_qty.cmp(&self.quantity().abs()) {
                    Less => {
                        balances.apply_pnl(-BaseOrQuote::PairedCurrency::pnl(
                            self.entry_price,
                            fill_price,
                            filled_qty,
                        ));
                        self.quantity += filled_qty;
                        assert2::debug_assert!(self.quantity < Zero::zero());
                    }
                    Equal => {
                        balances.apply_pnl(-BaseOrQuote::PairedCurrency::pnl(
                            self.entry_price,
                            fill_price,
                            filled_qty,
                        ));
                        self.quantity += filled_qty;
                        debug_assert_eq!(self.quantity, Zero::zero());
                        self.entry_price = Zero::zero();
                    }
                    Greater => {
                        balances.apply_pnl(-BaseOrQuote::PairedCurrency::pnl(
                            self.entry_price,
                            fill_price,
                            self.quantity.abs(),
                        ));
                        self.quantity += filled_qty;
                        assert2::debug_assert!(self.quantity > Zero::zero());
                        self.entry_price = fill_price;
                    }
                },
                Sell => {
                    self.entry_price = QuoteCurrency::new_weighted_price(
                        self.entry_price,
                        *self.quantity.abs().as_ref(),
                        fill_price,
                        *filled_qty.as_ref(),
                    );
                    self.quantity -= filled_qty;
                }
            },
        }
        debug_assert!({
            if self.quantity.is_zero() {
                self.entry_price.is_zero()
            } else {
                true
            }
        });
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for Position<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.quantity.is_zero() {
            write!(f, "Neutral")
        } else if self.quantity.is_negative() {
            write!(
                f,
                "Short {} @ {}",
                self.quantity().abs(),
                self.entry_price()
            )
        } else {
            write!(f, "Long {} @ {}", self.quantity(), self.entry_price())
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
        let pos = Position::new(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(-317, 3).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(958423665, 5).unwrap()),
        )
        .unwrap();
        assert_eq!(&pos.to_string(), "Short 0.31700 Base @ 9584.23665 Quote");
    }

    #[test]
    #[tracing_test::traced_test]
    fn position_change_position() {
        let qty = BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap());
        let entry_price = QuoteCurrency::from(Decimal::try_from_scaled(9584_23, 2).unwrap());

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));

        let mut pos = Position::new(-qty, entry_price).unwrap();

        let exit_price = QuoteCurrency::new(30204_27, 2);
        pos.change(qty, exit_price, Side::Buy, &mut balances);
        assert_eq!(pos, Position::default());
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
        let mut pos = Position::new(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(16800, 5).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(5949354994, 5).unwrap()),
        )
        .unwrap();
        let filled_qty = BaseCurrency::new(16800, 5);
        let fill_price = QuoteCurrency::new(6001260000, 5);
        let mut balances = Balances::new(QuoteCurrency::new(1000, 0));
        pos.change(filled_qty, fill_price, Side::Sell, &mut balances);
    }

    #[test]
    fn size_of_position() {
        assert_eq!(size_of::<Position<i32, 4, BaseCurrency<_, 4>>>(), 8);
        assert_eq!(size_of::<Position<i64, 5, BaseCurrency<_, 5>>>(), 16);
    }

    proptest! {
        #[test]
        fn position_change_proptest_neutral(qty in 1..100_i64, fill_price in 1..100_i64, do_buy in 0..2_i32) {
            let filled_qty = BaseCurrency::<i64, 5>::new(qty, 0);
            let fill_price = QuoteCurrency::new(fill_price, 0);

            let mut position = Position::default();
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
                Side::Buy => assert_eq!(position, Position::new(filled_qty, fill_price).unwrap()),
                Side::Sell => assert_eq!(position, Position::new(-filled_qty, fill_price).unwrap()),
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
            let mut position = Position::new(start_qty, fill_price).unwrap();
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Sell,
                &mut balances,
            );
            let new_qty = (start_qty - filled_qty).abs();
            if filled_qty > start_qty {
                assert_eq!(position, Position::new(-new_qty, fill_price).unwrap());
            } else if filled_qty < start_qty {
                assert_eq!(position, Position::new(new_qty, fill_price).unwrap());
            } else {
                assert_eq!(position, Position::default());
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
            let mut position = Position::new(start_qty, fill_price).unwrap();
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Buy,
                &mut balances,
            );
            let new_qty = start_qty + filled_qty;
            assert_eq!(position, Position::new(new_qty, fill_price).unwrap());
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
            let mut position = Position::new(-start_qty, fill_price).unwrap();
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Buy,
                &mut balances,
            );
            let new_qty = (start_qty - filled_qty).abs();
            if filled_qty > start_qty {
                assert_eq!(position, Position::new(new_qty, fill_price).unwrap());
            } else if filled_qty < start_qty {
                assert_eq!(position, Position::new(-new_qty, fill_price).unwrap());
            } else {
                assert_eq!(position, Position::default());
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
            let mut position = Position::new(-start_qty, fill_price).unwrap();
            let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));

            position.change(
                filled_qty,
                fill_price,
                Side::Sell,
                &mut balances,
            );
            let new_qty = start_qty + filled_qty;
            assert_eq!(position, Position::new(-new_qty, fill_price).unwrap());
            assert_eq!(balances, Balances::builder()
                .equity(QuoteCurrency::new(10_000, 0))
                .total_fees_paid(Zero::zero())
                .build()
            );
        }
    }
}
