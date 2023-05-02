use fpdec::Decimal;

use crate::{
    errors::{Error, Result},
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency},
};

#[derive(Debug, Clone, Default)]
/// Describes the position information of the account
pub struct Position<S>
where
    S: Currency + Default,
{
    /// The position size
    size: S,
    /// The entry price of the position
    entry_price: QuoteCurrency,
}

impl<S> Position<S>
where
    S: Currency + Default,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new position with all fields custom.
    /// NOTE: not usually called, but for advanced use cases
    ///
    /// # Panics:
    /// In debug mode, if inputs don't make sense
    #[must_use]
    pub fn new(size: S, entry_price: QuoteCurrency) -> Self {
        debug_assert!(entry_price >= quote!(0.0));

        Position { size, entry_price }
    }

    /// Return the position size
    #[inline(always)]
    pub fn size(&self) -> S {
        self.size
    }

    /// Return the entry price of the position
    #[inline(always)]
    pub fn entry_price(&self) -> QuoteCurrency {
        self.entry_price
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    #[inline(always)]
    pub fn unrealized_pnl(&self, bid: QuoteCurrency, ask: QuoteCurrency) -> S::PairedCurrency {
        // The upnl is based on the possible fill price, not the mid-price
        if self.size > S::new_zero() {
            S::PairedCurrency::pnl(self.entry_price, bid, self.size)
        } else {
            S::PairedCurrency::pnl(self.entry_price, ask, self.size)
        }
    }

    #[inline]
    pub(crate) fn open_position(&mut self, amount: S, price: QuoteCurrency) {
        self.size = amount;
        self.entry_price = price;
    }

    /// Add to a position
    pub(crate) fn increase_position(&mut self, amount: S, price: QuoteCurrency) -> Result<()> {
        if amount <= S::new_zero() {
            return Err(Error::InvalidAmount);
        }
        let new_size = self.size + amount;
        self.entry_price = QuoteCurrency::new(
            (self.entry_price * self.size.inner() + price * amount.inner()).inner()
                / new_size.inner(),
        );

        self.size = new_size;

        Ok(())
    }

    /// Reduce the position
    pub(crate) fn decrease_position(&mut self, amount: S, price: QuoteCurrency) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn increase_position() {
        let mut pos = Position::default();
        pos.open_position(quote!(100), quote!(100));
        pos.increase_position(quote!(150), quote!(120)).unwrap();
        assert_eq!(pos.size, quote!(250));
        assert_eq!(pos.entry_price, quote!(112));
    }
}
