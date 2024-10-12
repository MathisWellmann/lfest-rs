use std::{cmp::Ordering, ops::Neg};

use num_traits::Zero;
use tracing::debug;

use crate::{
    position_inner::PositionInner,
    prelude::{
        CurrencyMarker, Mon, Monies, Quote, TransactionAccounting, USER_POSITION_MARGIN_ACCOUNT,
    },
    types::{MarginCurrencyMarker, Side},
};

/// A futures position can be one of three variants.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub enum Position<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    // BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
{
    /// No position present.
    #[default]
    Neutral,
    /// A position in the long direction.
    Long(PositionInner<T, BaseOrQuote>),
    /// A position in the short direction.
    Short(PositionInner<T, BaseOrQuote>),
}

impl<T, BaseOrQuote> Position<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
{
    /// Return the positions unrealized profit and loss.
    pub fn unrealized_pnl(
        &self,
        bid: Monies<T, Quote>,
        ask: Monies<T, Quote>,
    ) -> Monies<T, BaseOrQuote::PairedCurrency> {
        match self {
            Position::Neutral => Monies::zero(),
            Position::Long(inner) => inner.unrealized_pnl(bid),
            Position::Short(inner) => inner.unrealized_pnl(ask).neg(),
        }
    }

    /// The quantity of the position, is negative when short.
    pub fn quantity(&self) -> Monies<T, BaseOrQuote> {
        match self {
            Position::Neutral => Monies::zero(),
            Position::Long(inner) => inner.quantity(),
            Position::Short(inner) => inner.quantity().neg(),
        }
    }

    /// Get the outstanding fees of the position that will be payed when reducing the position.
    pub fn outstanding_fees(&self) -> Monies<T, BaseOrQuote::PairedCurrency> {
        match self {
            Position::Neutral => Monies::zero(),
            Position::Long(inner) => inner.outstanding_fees(),
            Position::Short(inner) => inner.outstanding_fees(),
        }
    }

    /// The entry price of the position which is the total cost of the position relative to its quantity.
    pub fn entry_price(&self) -> Monies<T, Quote> {
        match self {
            Position::Neutral => Monies::zero(),
            Position::Long(inner) => inner.entry_price(),
            Position::Short(inner) => inner.entry_price(),
        }
    }

    /// The total value of the position which is composed of quantity and avg. entry price.
    pub fn total_cost(&self) -> Monies<T, BaseOrQuote::PairedCurrency> {
        match self {
            Position::Neutral => Monies::zero(),
            Position::Long(inner) => inner.total_cost(),
            Position::Short(inner) => inner.total_cost(),
        }
    }

    /// Change a position while doing proper accounting and balance transfers.
    pub(crate) fn change_position<Acc>(
        &mut self,
        filled_qty: Monies<T, BaseOrQuote>,
        fill_price: Monies<T, Quote>,
        side: Side,
        transaction_accounting: &mut Acc,
        init_margin_req: T,
        fees: Monies<T, BaseOrQuote::PairedCurrency>,
    ) where
        Acc: TransactionAccounting<T, BaseOrQuote::PairedCurrency>,
    {
        debug_assert!(
            filled_qty > Monies::zero(),
            "The filled_qty must be greater than zero"
        );
        debug!("old position: {}", self);
        match self {
            Position::Neutral => {
                assert_eq!(
                    transaction_accounting
                        .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                        .expect("Is valid account"),
                    Monies::zero()
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
                            T::one(),
                            fees,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            T::one(),
                            fees,
                        );
                        *self = Position::Neutral;
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Monies::zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_short_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            T::one(),
                            fees,
                        );
                        assert_eq!(inner.quantity(), Monies::zero());
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Monies::zero()
                        );
                        *self = Position::Short(PositionInner::new(
                            new_short_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Monies::zero(),
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
                            T::one().neg(),
                            fees,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            T::one().neg(),
                            fees,
                        );
                        *self = Position::Neutral;
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Monies::zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_long_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            T::one().neg(),
                            fees,
                        );
                        assert_eq!(inner.quantity(), Monies::zero());
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Monies::zero()
                        );
                        *self = Position::Long(PositionInner::new(
                            new_long_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Monies::zero(),
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
        // crate::utils::assert_user_wallet_balance(transaction_accounting);
        debug!("new position: {}", self);
    }
}

impl<T, BaseOrQuote> std::fmt::Display for Position<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    BaseOrQuote::PairedCurrency: MarginCurrencyMarker<T>,
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
