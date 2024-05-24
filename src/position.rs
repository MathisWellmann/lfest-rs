use fpdec::{Dec, Decimal};
use getset::{CopyGetters, Getters};
use tracing::trace;

use crate::{
    prelude::{
        Transaction, TransactionAccounting, USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT,
    },
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency, Side},
    utils::assert_user_wallet_balance,
};

/// A futures position can be one of three variants.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Position<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    /// No position present.
    Neutral,
    /// A position in the long direction.
    Long(PositionInner<Q>),
    /// A position in the short direction.
    Short(PositionInner<Q>),
}

impl<Q> Default for Position<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    fn default() -> Self {
        Position::Neutral
    }
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
            Position::Long(inner) => inner.unrealized_pnl(bid, Dec!(1)),
            Position::Short(inner) => inner.unrealized_pnl(ask, Dec!(-1)).into_negative(),
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
    ) where
        T: TransactionAccounting<Q::PairedCurrency>,
    {
        trace!("old position: {}", self);
        match self {
            Position::Neutral => match side {
                Side::Buy => {
                    *self = Position::Long(PositionInner::new(
                        filled_qty,
                        fill_price,
                        transaction_accounting,
                        init_margin_req,
                    ))
                }
                Side::Sell => {
                    *self = Position::Short(PositionInner::new(
                        filled_qty,
                        fill_price,
                        transaction_accounting,
                        init_margin_req,
                    ))
                }
            },
            Position::Long(inner) => match side {
                Side::Buy => {
                    inner.increase_contracts(
                        filled_qty,
                        fill_price,
                        transaction_accounting,
                        init_margin_req,
                    );
                }
                Side::Sell => {
                    if filled_qty > inner.quantity {
                        let pos_value = inner.quantity().convert(fill_price);
                        let new_short_qty = filled_qty - inner.quantity;
                        *self = Position::Short(PositionInner::new(
                            new_short_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                        ));
                    } else if filled_qty == inner.quantity {
                        inner.decrease_contracts(
                            filled_qty,
                            transaction_accounting,
                            init_margin_req,
                        );
                        *self = Position::Neutral;
                    } else {
                        inner.decrease_contracts(
                            filled_qty,
                            transaction_accounting,
                            init_margin_req,
                        );
                    }
                }
            },
            Position::Short(inner) => match side {
                Side::Buy => {
                    if filled_qty > inner.quantity {
                        let pos_value = inner.quantity().convert(fill_price);
                        let new_long_qty = filled_qty - inner.quantity;
                        *self = Position::Long(PositionInner::new(
                            new_long_qty,
                            fill_price,
                            transaction_accounting,
                            init_margin_req,
                        ));
                    } else if filled_qty == inner.quantity {
                        inner.decrease_contracts(
                            filled_qty,
                            transaction_accounting,
                            init_margin_req,
                        );
                        *self = Position::Neutral;
                    } else {
                        inner.decrease_contracts(
                            filled_qty,
                            transaction_accounting,
                            init_margin_req,
                        );
                    }
                }
                Side::Sell => {
                    inner.increase_contracts(
                        filled_qty,
                        fill_price,
                        transaction_accounting,
                        init_margin_req,
                    );
                }
            },
        };
        assert_user_wallet_balance(transaction_accounting);
        trace!("new position: {}", self);
    }
}

/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
#[derive(Debug, Clone, Default, Eq, PartialEq, Getters, CopyGetters)]
pub struct PositionInner<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    /// The number of futures contracts making up the position.
    /// Denoted in the currency in which the size is valued.
    /// e.g.: XBTUSD has a contract size of 1 USD, so `M::PairedCurrency` is USD.
    #[getset(get_copy = "pub")]
    quantity: Q,

    /// The entry price of the position.
    #[getset(get_copy = "pub")]
    entry_price: QuoteCurrency,
}

impl<Q> PositionInner<Q>
where
    Q: Currency,
    Q::PairedCurrency: MarginCurrency,
{
    /// Create a new instance.
    ///
    /// # Panics:
    /// if `quantity` or `entry_price` are invalid.
    pub fn new<T>(
        quantity: Q,
        entry_price: QuoteCurrency,
        accounting: &mut T,
        init_margin_req: Decimal,
    ) -> Self
    where
        T: TransactionAccounting<Q::PairedCurrency>,
    {
        assert!(quantity > Q::new_zero());
        assert!(entry_price > quote!(0));

        let margin = quantity.convert(entry_price) * init_margin_req;
        let transaction =
            Transaction::new(USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT, margin);
        accounting.create_margin_transfer(transaction);

        Self {
            quantity,
            entry_price,
        }
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    pub fn unrealized_pnl(
        &self,
        mark_to_market_price: QuoteCurrency,
        direction_multiplier: Decimal,
    ) -> Q::PairedCurrency {
        debug_assert!(
            direction_multiplier == Dec!(1) || direction_multiplier == Dec!(-1),
            "Multiplier must be one of those."
        );
        Q::PairedCurrency::pnl(
            self.entry_price,
            mark_to_market_price,
            self.quantity * direction_multiplier,
        )
    }

    /// The total position value including unrealized profit and loss.
    /// Denoted in the margin `Currency`.
    pub fn value(
        &self,
        mark_to_market_price: QuoteCurrency,
        direction_multiplier: Decimal,
    ) -> Q::PairedCurrency {
        self.quantity.convert(self.entry_price)
            + self.unrealized_pnl(mark_to_market_price, direction_multiplier)
    }

    /// Add contracts to the position.
    pub(crate) fn increase_contracts<T>(
        &mut self,
        to_add: Q,
        entry_price: QuoteCurrency,
        accounting: &mut T,
        init_margin_req: Decimal,
    ) where
        T: TransactionAccounting<Q::PairedCurrency>,
    {
        debug_assert!(to_add > Q::new_zero());
        debug_assert!(entry_price > quote!(0));

        self.quantity += to_add;
        self.entry_price = self.new_avg_entry_price(to_add, entry_price);

        let margin = to_add.convert(entry_price) * init_margin_req;
        let transaction =
            Transaction::new(USER_POSITION_MARGIN_ACCOUNT, USER_WALLET_ACCOUNT, margin);
        accounting.create_margin_transfer(transaction);
    }

    /// Decrease the position.
    pub(crate) fn decrease_contracts<T>(
        &mut self,
        to_subtract: Q,
        accounting: &mut T,
        init_marign_req: Decimal,
    ) where
        T: TransactionAccounting<Q::PairedCurrency>,
    {
        debug_assert!(to_subtract > Q::new_zero());
        debug_assert!(to_subtract <= self.quantity);

        self.quantity -= to_subtract;
        debug_assert!(self.quantity >= Q::new_zero());

        todo!("accounting")
    }

    /// Compute the new entry price of the position when some quantity is added at a specifiy `entry_price`.
    fn new_avg_entry_price(&self, added_qty: Q, entry_price: QuoteCurrency) -> QuoteCurrency {
        debug_assert!(added_qty > Q::new_zero());
        debug_assert!(entry_price > quote!(0));

        let new_qty = self.quantity + added_qty;
        QuoteCurrency::new(
            ((*self.quantity.as_ref() * *self.entry_price.as_ref())
                + (*added_qty.as_ref() * *entry_price.as_ref()))
                / *new_qty.as_ref(),
        )
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
            Position::Long(inner) => write!(f, "Long {} @ {}", inner.quantity, inner.entry_price),
            Position::Short(inner) => write!(f, "Short {} @ {}", inner.quantity, inner.entry_price),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, prelude::InMemoryTransactionAccounting};

    #[test]
    fn position_inner_new_avg_entry_price() {
        let pos = PositionInner {
            quantity: base!(0.1),
            entry_price: quote!(100),
        };
        assert_eq!(pos.new_avg_entry_price(base!(0.1), quote!(50)), quote!(75));
        assert_eq!(pos.new_avg_entry_price(base!(0.1), quote!(90)), quote!(95));
        assert_eq!(
            pos.new_avg_entry_price(base!(0.1), quote!(150)),
            quote!(125)
        );
        assert_eq!(
            pos.new_avg_entry_price(base!(0.3), quote!(200)),
            quote!(175)
        );
    }

    #[test]
    fn change_position_from_neutral() {
        let mut pos = Position::Neutral;
        let filled_qty = base!(1);
        let fill_price = quote!(100);
        let side = Side::Buy;
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        let init_margin_req = Dec!(1);
        pos.change_position(
            filled_qty,
            fill_price,
            side,
            &mut accounting,
            init_margin_req,
        );
        let mut accounting = InMemoryTransactionAccounting::new(quote!(1000));
        assert_eq!(
            pos,
            Position::Long(PositionInner::new(
                filled_qty,
                fill_price,
                &mut accounting,
                init_margin_req
            ))
        );
        assert_eq!(
            accounting.margin_balance_of(USER_WALLET_ACCOUNT).unwrap(),
            quote!(900)
        );
    }
}
