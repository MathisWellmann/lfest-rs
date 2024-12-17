use std::{cmp::Ordering, ops::Neg};

use const_decimal::Decimal;
use num_traits::Zero;

use crate::{
    position_inner::PositionInner,
    prelude::{Currency, Mon, QuoteCurrency, TransactionAccounting, USER_POSITION_MARGIN_ACCOUNT},
    types::{MarginCurrency, Side},
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

    /// Get the outstanding fees of the position that will be payed when reducing the position.
    pub fn outstanding_fees(&self) -> BaseOrQuote::PairedCurrency {
        match self {
            Position::Neutral => BaseOrQuote::PairedCurrency::zero(),
            Position::Long(inner) => inner.outstanding_fees(),
            Position::Short(inner) => inner.outstanding_fees(),
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
    #[tracing::instrument(level = "debug")]
    pub(crate) fn change_position<Acc>(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        transaction_accounting: &mut Acc,
        init_margin_req: Decimal<I, D>,
        fees: BaseOrQuote::PairedCurrency,
    ) where
        Acc: TransactionAccounting<I, D, BaseOrQuote::PairedCurrency>,
    {
        assert2::debug_assert!(
            filled_qty > BaseOrQuote::zero(),
            "The filled_qty must be greater than zero"
        );
        match self {
            Position::Neutral => {
                debug_assert_eq!(
                    transaction_accounting
                        .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                        .expect("Is valid account"),
                    BaseOrQuote::PairedCurrency::zero()
                );
                match side {
                    Side::Buy => {
                        *self = Position::Long(PositionInner::new(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            fees,
                        ))
                    }
                    Side::Sell => {
                        *self = Position::Short(PositionInner::new(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            fees,
                        ))
                    }
                }
            }
            Position::Long(inner) => match side {
                Side::Buy => {
                    inner.increase_contracts(
                        filled_qty,
                        fill_price,
                        transaction_accounting,
                        init_margin_req,
                        fees,
                    );
                }
                Side::Sell => match filled_qty.cmp(&inner.quantity()) {
                    Ordering::Less => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            1,
                            fees,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            1,
                            fees,
                        );
                        *self = Position::Neutral;
                        debug_assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            BaseOrQuote::PairedCurrency::zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_short_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            1,
                            fees,
                        );
                        assert_eq!(inner.quantity(), BaseOrQuote::zero());
                        debug_assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            BaseOrQuote::PairedCurrency::zero()
                        );
                        *self = Position::Short(PositionInner::new(
                            new_short_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            BaseOrQuote::PairedCurrency::zero(),
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
                            transaction_accounting,
                            init_margin_req,
                            -1,
                            fees,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            -1,
                            fees,
                        );
                        *self = Position::Neutral;
                        debug_assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            BaseOrQuote::PairedCurrency::zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_long_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            -1,
                            fees,
                        );
                        assert_eq!(inner.quantity(), BaseOrQuote::zero());
                        debug_assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            BaseOrQuote::PairedCurrency::zero()
                        );
                        *self = Position::Long(PositionInner::new(
                            new_long_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            BaseOrQuote::PairedCurrency::zero(),
                        ));
                    }
                },
                Side::Sell => {
                    inner.increase_contracts(
                        filled_qty,
                        fill_price,
                        transaction_accounting,
                        init_margin_req,
                        fees,
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
    use crate::{prelude::*, MockTransactionAccounting};

    #[test]
    #[tracing_test::traced_test]
    fn position_change_position() {
        let mut pos = Position::Short(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(317, 3).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(958423665, 5).unwrap()),
            QuoteCurrency::zero(),
        ));
        let mut acc = MockTransactionAccounting::default();
        let init_margin_req = Decimal::ONE;
        let fees = QuoteCurrency::zero();
        pos.change_position(
            BaseCurrency::new(317, 3),
            QuoteCurrency::new(3020427, 2),
            Side::Buy,
            &mut acc,
            init_margin_req,
            fees,
        );
    }

    #[test]
    #[tracing_test::traced_test]
    fn position_change_position_2() {
        use crate::accounting::TAccount;

        let mut pos = Position::Long(PositionInner::from_parts(
            BaseCurrency::<i64, 5>::from(Decimal::try_from_scaled(16800, 5).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(5949354994, 5).unwrap()),
            QuoteCurrency::from(Decimal::try_from_scaled(600056, 5).unwrap()),
        ));
        let filled_qty = BaseCurrency::new(16800, 5);
        let fill_price = QuoteCurrency::new(6001260000, 5);
        let mut acc = InMemoryTransactionAccounting::from_accounts([
            TAccount::from_parts(
                QuoteCurrency::from(Decimal::try_from_scaled(1499293480, 5).unwrap()),
                QuoteCurrency::from(Decimal::try_from_scaled(1498785120, 5).unwrap()),
            ),
            TAccount::from_parts(
                QuoteCurrency::from(Decimal::try_from_scaled(499293480, 5).unwrap()),
                QuoteCurrency::from(Decimal::try_from_scaled(499293480, 5).unwrap()),
            ),
            TAccount::from_parts(
                QuoteCurrency::from(Decimal::try_from_scaled(999491640, 5).unwrap()),
                QuoteCurrency::from(Decimal::try_from_scaled(0, 0).unwrap()),
            ),
            TAccount::from_parts(
                QuoteCurrency::from(Decimal::try_from_scaled(0, 5).unwrap()),
                QuoteCurrency::from(Decimal::try_from_scaled(0, 0).unwrap()),
            ),
            TAccount::from_parts(
                QuoteCurrency::from(Decimal::try_from_scaled(0, 5).unwrap()),
                QuoteCurrency::from(Decimal::try_from_scaled(0, 0).unwrap()),
            ),
            TAccount::from_parts(
                QuoteCurrency::from(Decimal::try_from_scaled(0, 5).unwrap()),
                QuoteCurrency::from(Decimal::try_from_scaled(1000000000, 5).unwrap()),
            ),
        ]);
        let init_margin_req = Decimal::ONE;
        let fees = QuoteCurrency::zero();
        pos.change_position(
            filled_qty,
            fill_price,
            Side::Sell,
            &mut acc,
            init_margin_req,
            fees,
        );
    }
}
