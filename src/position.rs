use fpdec::{Dec, Decimal};
use getset::{CopyGetters, Getters};
use tracing::trace;

use crate::{
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency, Side},
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

    pub(crate) fn change_position(&mut self, filled_qty: Q, fill_price: QuoteCurrency, side: Side) {
        trace!("old position: {}", self);
        match self {
            Position::Neutral => match side {
                Side::Buy => *self = Position::Long(PositionInner::new(filled_qty, fill_price)),
                Side::Sell => *self = Position::Short(PositionInner::new(filled_qty, fill_price)),
            },
            Position::Long(inner) => match side {
                Side::Buy => inner.increase_contracts(filled_qty, fill_price),
                Side::Sell => {
                    if filled_qty > inner.quantity {
                        let new_short_qty = filled_qty - inner.quantity;
                        *self = Position::Short(PositionInner::new(new_short_qty, fill_price));
                    } else if filled_qty == inner.quantity {
                        *self = Position::Neutral;
                    } else {
                        inner.decrease_contracts(filled_qty);
                    }
                }
            },
            Position::Short(inner) => match side {
                Side::Buy => {
                    if filled_qty > inner.quantity {
                        let new_long_qty = filled_qty - inner.quantity;
                        *self = Position::Long(PositionInner::new(new_long_qty, fill_price));
                    } else if filled_qty == inner.quantity {
                        *self = Position::Neutral;
                    } else {
                        inner.decrease_contracts(filled_qty);
                    }
                }
                Side::Sell => inner.increase_contracts(filled_qty, fill_price),
            },
        }
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
    pub fn new(quantity: Q, entry_price: QuoteCurrency) -> Self {
        assert!(quantity > Q::new_zero());
        assert!(entry_price > quote!(0));
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
    pub(crate) fn increase_contracts(&mut self, to_add: Q, entry_price: QuoteCurrency) {
        debug_assert!(to_add > Q::new_zero());
        debug_assert!(entry_price > quote!(0));

        self.quantity += to_add;
        self.entry_price = self.new_avg_entry_price(to_add, entry_price);
    }

    /// Decrease the position.
    pub(crate) fn decrease_contracts(&mut self, to_subtract: Q) {
        debug_assert!(to_subtract > Q::new_zero());
        debug_assert!(to_subtract <= self.quantity);

        self.quantity -= to_subtract;
        debug_assert!(self.quantity >= Q::new_zero());
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
    use crate::base;

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
}
