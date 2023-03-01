use malachite::Rational;
use serde::{Deserialize, Serialize};

use crate::{quote, Currency, FuturesTypes, Leverage, QuoteCurrency};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
where S: Currency
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
    pub fn entry_price(&self) -> &QuoteCurrency {
        &self.entry_price
    }

    /// Return the positions leverage
    #[inline(always)]
    pub fn leverage(&self) -> &Leverage {
        &self.leverage
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    #[inline(always)]
    pub fn unrealized_pnl(&self) -> &S::PairedCurrency {
        &self.unrealized_pnl
    }

    /// Change the position size by a given delta at a certain price
    pub(crate) fn change_size(
        &mut self,
        size_delta: &S,
        price: &QuoteCurrency,
        futures_type: FuturesTypes,
    ) {
        trace!("change_size({}, {}, {})", size_delta, price, futures_type);

        if self.size > S::new_zero() {
            if self.size + size_delta < S::new_zero() {
                // counts as new position as all old position size is sold
                self.entry_price = price.clone();
            } else if (self.size + size_delta).abs() > self.size {
                let size_abs = self.size.abs().inner();
                let size_delta_abs = size_delta.abs().inner();
                let entry_price: Rational = ((size_abs * self.entry_price.inner())
                    + (size_delta_abs * price.inner()))
                    / (size_abs + size_delta_abs);
                self.entry_price = QuoteCurrency::new(entry_price);
            }
        } else if self.size < S::new_zero() {
            if self.size + size_delta > S::new_zero() {
                self.entry_price = price.clone();
            } else if self.size + size_delta < self.size {
                let size_abs = self.size.abs().inner();
                let size_delta_abs = size_delta.abs().inner();
                let entry_price = self.entry_price.inner();
                let entry_price: Rational = ((size_abs * entry_price)
                    + (size_delta_abs * price.inner()))
                    / (size_abs + size_delta_abs);
                self.entry_price = QuoteCurrency::new(entry_price);
            }
        } else {
            self.entry_price = price.clone();
        }
        self.size += size_delta.clone();

        self.update_state(&price, futures_type);
    }

    /// Update the state to reflect price changes
    pub(crate) fn update_state(&mut self, price: &QuoteCurrency, futures_type: FuturesTypes) {
        self.unrealized_pnl = if self.size != S::new_zero() {
            futures_type.pnl(&self.entry_price, price, self.size)
        } else {
            S::PairedCurrency::new_zero()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, BaseCurrency};

    #[test]
    fn position_change_size() {
        let mut pos = Position::new_init(1.0);
        let futures_type = FuturesTypes::Inverse;

        pos.change_size(quote!(100.0), quote!(100.0), futures_type);
        assert_eq!(pos.size, quote!(100.0));
        assert_eq!(pos.entry_price, quote!(100.0));
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, base!(0.0));

        pos.change_size(quote!(-50.0), quote!(150.0), futures_type);
        assert_eq!(pos.size, quote!(50.0));
        assert_eq!(pos.entry_price, quote!(100.0));
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl.into_rounded(2), base!(0.17));

        pos.change_size(quote!(50.0), quote!(150.0), futures_type);
        assert_eq!(pos.size, quote!(100.0));
        assert_eq!(pos.entry_price, quote!(125.0));
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl.into_rounded(2), base!(0.13));

        pos.change_size(quote!(-150.0), quote!(150.0), futures_type);
        assert_eq!(pos.size, quote!(-50.0));
        assert_eq!(pos.entry_price, quote!(150.0));
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, base!(0.0));

        pos.change_size(quote!(50.0), quote!(150.0), futures_type);
        assert_eq!(pos.size, quote!(0.0));
        assert_eq!(pos.entry_price, quote!(150.0));
        assert_eq!(pos.leverage, 1.0);
        assert_eq!(pos.unrealized_pnl, base!(0.0));
    }

    #[test]
    fn position_update_state_inverse_futures() {
        let mut pos = Position::new(quote!(100.0), quote!(100.0), 1.0, base!(0.0));
        pos.update_state(quote!(110.0), FuturesTypes::Inverse);

        assert_eq!(pos.unrealized_pnl.into_rounded(2), base!(0.09));
    }

    #[test]
    fn position_update_state_linear_futures() {
        let mut pos = Position::new(base!(1.0), quote!(100.0), 1.0, quote!(0.0));
        pos.update_state(quote!(110.0), FuturesTypes::Linear);

        assert_eq!(pos.unrealized_pnl.into_rounded(2), quote!(10.0));
    }
}
