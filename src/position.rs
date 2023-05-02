use fpdec::Decimal;

use crate::{
    errors::{Error, Result},
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency},
};

#[derive(Debug, Clone, Default)]
/// Describes the position information of the account
pub struct Position<S> {
    /// The number of futures contracts making up the position.
    size: S,
    /// The entry price of the position
    entry_price: QuoteCurrency,
}

impl<S> Position<S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
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

    /// Create a new position with all fields custom.
    ///
    /// # Arguments:
    /// `size`: The position size, negative denoting a negative position.
    /// `entry_price`: The price at which the position was entered.
    ///
    pub(crate) fn open_position(&mut self, size: S, price: QuoteCurrency) -> Result<()> {
        if price <= quote!(0) {
            return Err(Error::InvalidPrice);
        }
        self.size = size;
        self.entry_price = price;

        Ok(())
    }

    /// Add to a position.
    pub(crate) fn increase_long_position(&mut self, amount: S, price: QuoteCurrency) -> Result<()> {
        if amount < S::new_zero() {
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

    /// Reduce a long position.
    ///
    /// # Arguments:
    /// `amount`: The amount to decrease the position by, must be smaller or equal to the position size.
    /// `price`: The price at which it is sold.
    ///
    /// # Returns:
    /// If Ok, the net realized profit and loss for that specific futures contract.
    pub(crate) fn decrease_long_position(
        &mut self,
        amount: S,
        price: QuoteCurrency,
    ) -> Result<S::PairedCurrency> {
        if amount < S::new_zero() || amount > self.size {
            return Err(Error::InvalidAmount);
        }
        self.size = self.size - amount;

        Ok(S::PairedCurrency::pnl(self.entry_price, price, amount))
    }

    /// Increase a short position
    pub(crate) fn increase_short_position(
        &mut self,
        amount: S,
        price: QuoteCurrency,
    ) -> Result<()> {
        todo!()
    }

    /// Reduce a short position
    pub(crate) fn decrease_short_position(
        &mut self,
        amount: S,
        price: QuoteCurrency,
    ) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn increase_long_position() {
        let mut pos = Position::default();
        pos.open_position(quote!(100), quote!(100)).unwrap();
        pos.increase_long_position(quote!(150), quote!(120))
            .unwrap();
        assert_eq!(pos.size, quote!(250));
        assert_eq!(pos.entry_price, quote!(112));
    }

    #[test]
    fn decrease_long_position() {
        let mut pos = Position::default();
        pos.open_position(base!(1), quote!(150)).unwrap();
        assert!(pos.decrease_long_position(base!(1.1), quote!(150)).is_err());
        assert_eq!(
            pos.decrease_long_position(base!(0.5), quote!(160)).unwrap(),
            quote!(5)
        );
        assert_eq!(pos.entry_price, quote!(150));
        assert_eq!(pos.size, base!(0.5));
    }
}
