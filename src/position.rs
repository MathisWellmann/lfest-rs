use fpdec::Decimal;

use crate::{
    quote,
    types::{Currency, Leverage, MarginCurrency, QuoteCurrency},
};

#[derive(Debug, Clone, Default)]
/// Describes the position information of the account
pub struct Position<S>
where S: Currency
{
    /// The position size
    size: S,
    /// The entry price of the position
    entry_price: QuoteCurrency,
    /// The current position leverage
    leverage: Leverage,
    /// The currently unrealized profit and loss
    unrealized_pnl: S::PairedCurrency,
}

impl<S> Position<S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new, initially neutral, position with a given leverage
    #[must_use]
    #[inline]
    pub fn new_init(leverage: Leverage) -> Self {
        Position {
            size: S::new_zero(),
            entry_price: quote!(0.0),
            leverage,
            unrealized_pnl: S::PairedCurrency::new_zero(),
        }
    }

    /// Create a new position with all fields custom.
    /// NOTE: not usually called, but for advanced use cases
    ///
    /// # Panics:
    /// In debug mode, if inputs don't make sense
    #[must_use]
    pub fn new(
        size: S,
        entry_price: QuoteCurrency,
        leverage: Leverage,
        unrealized_pnl: S::PairedCurrency,
    ) -> Self {
        debug_assert!(entry_price >= quote!(0.0));

        Position {
            size,
            entry_price,
            leverage,
            unrealized_pnl,
        }
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

    /// Return the positions leverage
    #[inline(always)]
    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    #[inline(always)]
    pub fn unrealized_pnl(&self) -> S::PairedCurrency {
        self.unrealized_pnl
    }

    /// Change the position size by a given delta at a certain price
    pub(crate) fn change_size(&mut self, size_delta: S, price: QuoteCurrency) {
        trace!("change_size({}, {})", size_delta, price);

        if self.size > S::new_zero() {
            if self.size + size_delta < S::new_zero() {
                // counts as new position as all old position size is sold
                self.entry_price = price;
            } else if (self.size + size_delta).abs() > self.size {
                let entry_price = ((self.entry_price * self.size.abs().inner())
                    + (price * size_delta.abs().inner()))
                    / QuoteCurrency::new((self.size.abs() + size_delta.abs()).inner());
                self.entry_price = entry_price;
            }
        } else if self.size < S::new_zero() {
            if self.size + size_delta > S::new_zero() {
                self.entry_price = price;
            } else if self.size + size_delta < self.size {
                let size_abs = self.size.abs().inner();
                let size_delta_abs = size_delta.abs().inner();
                let entry_price = self.entry_price.inner();
                let entry_price = ((size_abs * entry_price) + (size_delta_abs * price.inner()))
                    / (size_abs + size_delta_abs);
                self.entry_price = QuoteCurrency::new(entry_price);
            }
        } else {
            self.entry_price = price;
        }
        self.size += size_delta;

        self.unrealized_pnl = S::PairedCurrency::pnl(self.entry_price, price, self.size);
    }

    /// Update the state to reflect price changes
    #[inline(always)]
    pub(crate) fn update_state(&mut self, bid: QuoteCurrency, ask: QuoteCurrency) {
        // The upnl is based on the possible fill price, not the mid-price
        if self.size > S::new_zero() {
            self.unrealized_pnl = S::PairedCurrency::pnl(self.entry_price, bid, self.size);
        } else {
            self.unrealized_pnl = S::PairedCurrency::pnl(self.entry_price, ask, self.size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn position_change_size_inverse() {
        let mut pos = Position::new_init(leverage!(1.0));

        pos.change_size(quote!(100.0), quote!(100.0));
        assert_eq!(pos.size, quote!(100.0));
        assert_eq!(pos.entry_price, quote!(100.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, base!(0.0));

        pos.change_size(quote!(-50.0), quote!(125.0));
        assert_eq!(pos.size, quote!(50.0));
        assert_eq!(pos.entry_price, quote!(100.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, base!(0.1));
    }

    #[test]
    fn position_change_size_linear() {
        let mut pos = Position::new_init(leverage!(1.0));

        pos.change_size(base!(1.0), quote!(100.0));
        assert_eq!(pos.size, base!(1.0));
        assert_eq!(pos.entry_price, quote!(100.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, quote!(0.0));

        pos.change_size(base!(-0.5), quote!(150.0));
        assert_eq!(pos.size, base!(0.5));
        assert_eq!(pos.entry_price, quote!(100.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, quote!(25));

        pos.change_size(base!(0.5), quote!(150.0));
        assert_eq!(pos.size, base!(1.0));
        assert_eq!(pos.entry_price, quote!(125.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, quote!(25));

        pos.change_size(base!(-1.5), quote!(150.0));
        assert_eq!(pos.size, base!(-0.5));
        assert_eq!(pos.entry_price, quote!(150.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, quote!(0.0));

        pos.change_size(base!(0.5), quote!(150.0));
        assert_eq!(pos.size, base!(0.0));
        assert_eq!(pos.entry_price, quote!(150.0));
        assert_eq!(pos.leverage, leverage!(1.0));
        assert_eq!(pos.unrealized_pnl, quote!(0.0));
    }

    #[test]
    fn position_update_state_inverse_futures() {
        let mut pos = Position::new(quote!(100.0), quote!(100.0), leverage!(1.0), base!(0.0));
        pos.update_state(quote!(125.0), quote!(125.1));

        assert_eq!(pos.unrealized_pnl, base!(0.2));
    }

    #[test]
    fn position_update_state_linear_futures() {
        let mut pos = Position::new(base!(1.0), quote!(100.0), leverage!(1.0), quote!(0.0));
        pos.update_state(quote!(110.0), quote!(110.1));

        assert_eq!(pos.unrealized_pnl, quote!(10.0));
    }
}
