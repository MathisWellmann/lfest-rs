use std::cmp::Ordering;

use fpdec::{Dec, Decimal};
use tracing::debug;

use crate::{
    position_inner::PositionInner,
    prelude::{TransactionAccounting, USER_POSITION_MARGIN_ACCOUNT},
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency, Side},
};

/// A futures position can be one of three variants.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub enum Position<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    /// No position present.
    #[default]
    Neutral,
    /// A position in the long direction.
    Long(PositionInner<Q>),
    /// A position in the short direction.
    Short(PositionInner<Q>),
}

impl<Q> Position<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    /// Return the positions unrealized profit and loss.
    pub fn unrealized_pnl(&self, bid: QuoteCurrency, ask: QuoteCurrency) -> Q::PairedCurrency {
        match self {
            Position::Neutral => Q::PairedCurrency::new_zero(),
            Position::Long(inner) => inner.unrealized_pnl(bid),
            Position::Short(inner) => inner.unrealized_pnl(ask).into_negative(),
        }
    }

    /// The quantity of the position, is negative when short.
    pub fn quantity(&self) -> Q {
        match self {
            Position::Neutral => Q::new_zero(),
            Position::Long(inner) => inner.quantity(),
            Position::Short(inner) => inner.quantity().into_negative(),
        }
    }

    /// Get the outstanding fees of the position that will be payed when reducing the position.
    pub fn outstanding_fees(&self) -> Q::PairedCurrency {
        match self {
            Position::Neutral => Q::PairedCurrency::new_zero(),
            Position::Long(inner) => inner.outstanding_fees(),
            Position::Short(inner) => inner.outstanding_fees(),
        }
    }

    /// The entry price of the position which is the total cost of the position relative to its quantity.
    pub fn entry_price(&self) -> QuoteCurrency {
        match self {
            Position::Neutral => quote!(0),
            Position::Long(inner) => inner.entry_price(),
            Position::Short(inner) => inner.entry_price(),
        }
    }

    /// The total value of the position which is composed of quantity and avg. entry price.
    pub fn total_cost(&self) -> Q::PairedCurrency {
        match self {
            Position::Neutral => Q::PairedCurrency::new_zero(),
            Position::Long(inner) => inner.total_cost(),
            Position::Short(inner) => inner.total_cost(),
        }
    }

    /// Change a position while doing proper accounting and balance transfers.
    pub(crate) fn change_position<T>(
        &mut self,
        filled_qty: Q,
        fill_price: QuoteCurrency,
        side: Side,
        transaction_accounting: &mut T,
        init_margin_req: Decimal,
        fees: Q::PairedCurrency,
    ) where
        T: TransactionAccounting<Q::PairedCurrency>,
    {
        debug_assert!(
            filled_qty > Q::new_zero(),
            "The filled_qty must be greater than zero"
        );
        debug!("old position: {}", self);
        match self {
            Position::Neutral => {
                assert_eq!(
                    transaction_accounting
                        .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                        .expect("Is valid account"),
                    Q::PairedCurrency::new_zero()
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
                            Dec!(1),
                            fees,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Dec!(1),
                            fees,
                        );
                        *self = Position::Neutral;
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Q::PairedCurrency::new_zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_short_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Dec!(1),
                            fees,
                        );
                        assert_eq!(inner.quantity(), Q::new_zero());
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Q::PairedCurrency::new_zero()
                        );
                        *self = Position::Short(PositionInner::new(
                            new_short_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Q::PairedCurrency::new_zero(),
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
                            Dec!(-1),
                            fees,
                        );
                    }
                    Ordering::Equal => {
                        inner.decrease_contracts(
                            filled_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Dec!(-1),
                            fees,
                        );
                        *self = Position::Neutral;
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Q::PairedCurrency::new_zero()
                        );
                    }
                    Ordering::Greater => {
                        let new_long_qty = filled_qty - inner.quantity();
                        inner.decrease_contracts(
                            inner.quantity(),
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Dec!(-1),
                            fees,
                        );
                        assert_eq!(inner.quantity(), Q::new_zero());
                        assert_eq!(
                            transaction_accounting
                                .margin_balance_of(USER_POSITION_MARGIN_ACCOUNT)
                                .expect("Is valid account"),
                            Q::PairedCurrency::new_zero()
                        );
                        *self = Position::Long(PositionInner::new(
                            new_long_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                            Q::PairedCurrency::new_zero(),
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

impl<Q> std::fmt::Display for Position<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
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
