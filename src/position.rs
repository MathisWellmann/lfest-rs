use fpdec::Decimal;

use crate::{
    errors::{Error, Result},
    quote,
    types::{Currency, MarginCurrency, QuoteCurrency},
};

#[derive(Debug, Clone, Default, Eq, PartialEq)]
/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
pub struct Position<M> {
    /// The number of futures contracts making up the position.
    size: i64,
    /// The entry price of the position
    entry_price: QuoteCurrency,
    /// The position margin of account, same denotation as wallet_balance
    position_margin: M,
    /// The order margin of account, same denotation as wallet_balance
    order_margin: M,
}

impl<M> Position<M>
where
    M: Currency + MarginCurrency,
{
    #[cfg(test)]
    pub(crate) fn new(
        size: i64,
        entry_price: QuoteCurrency,
        position_margin: M,
        order_margin: M,
    ) -> Self {
        Self {
            size,
            entry_price,
            position_margin,
            order_margin,
        }
    }

    /// Return the position size
    #[inline(always)]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Return the entry price of the position
    #[inline(always)]
    pub fn entry_price(&self) -> QuoteCurrency {
        self.entry_price
    }

    /// Return the collateral backing this position
    #[inline(always)]
    pub fn position_margin(&self) -> M {
        self.position_margin
    }

    /// Return the locked order margin
    #[inline(always)]
    pub fn order_margin(&self) -> M {
        self.order_margin
    }

    /// Returns the implied leverage of the position based on the position value and the collateral backing it.
    pub fn implied_leverage(&self, price: QuoteCurrency) -> f64 {
        todo!()
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    pub fn unrealized_pnl(&self, bid: QuoteCurrency, ask: QuoteCurrency) -> M {
        // The upnl is based on the possible fill price, not the mid-price, which is more conservative
        if self.size > 0 {
            M::pnl(self.entry_price, bid, self.size)
        } else {
            M::pnl(self.entry_price, ask, self.size)
        }
    }

    /// Create a new position with all fields custom.
    ///
    /// # Arguments:
    /// `size`: The position size, negative denoting a negative position.
    ///     The `size` must have been approved by the `RiskEngine`.
    /// `entry_price`: The price at which the position was entered.
    /// `position_margin`: The collateral backing this position.
    ///
    pub(crate) fn open_position(
        &mut self,
        size: u64,
        price: QuoteCurrency,
        position_margin: M,
    ) -> Result<()> {
        if price <= quote!(0) {
            return Err(Error::InvalidPrice);
        }
        self.size = size;
        self.entry_price = price;

        Ok(())
    }

    /// Increase a long (or neutral) position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The price at which it is sold.
    ///
    pub(crate) fn increase_long(&mut self, amount: u64, price: QuoteCurrency) -> Result<()> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if self.size < 0 {
            return Err(Error::OpenShort);
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
    pub(crate) fn decrease_long(&mut self, amount: u64, price: QuoteCurrency) -> Result<M> {
        if self.size < 0 {
            return Err(Error::OpenShort);
        }
        if amount <= 0 || amount as _ > self.size {
            return Err(Error::InvalidAmount);
        }
        self.size -= amount as _;

        Ok(M::pnl(self.entry_price, price, amount))
    }

    /// Increase a short position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the short position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The entry price.
    pub(crate) fn increase_short(&mut self, amount: u64, price: QuoteCurrency) -> Result<()> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if self.size > 0 {
            return Err(Error::OpenLong);
        }

        let new_size = self.size - amount as _;
        self.entry_price = QuoteCurrency::new(
            (self.entry_price * self.size.abs() + price * amount).inner() / new_size.inner().abs(),
        );
        self.size = new_size;

        Ok(())
    }

    /// Reduce a short position
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to decrease the short position by.
    ///     Must be smaller or equal to the open position size.
    /// `price`: The entry price.
    ///
    /// # Returns:
    /// If Ok, the net realized profit and loss for that specific futures contract.
    pub(crate) fn decrease_short(&mut self, amount: u64, price: QuoteCurrency) -> Result<M> {
        if self.size >= 0 {
            return Err(Error::OpenLong);
        }
        if amount <= 0 || amount.into_negative() < self.size {
            return Err(Error::InvalidAmount);
        }
        self.size = self.size + amount;

        Ok(M::pnl(self.entry_price, price, amount.into_negative()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn increase_long_position() {
        let mut pos = Position::default();
        pos.increase_long(quote!(150), quote!(120)).unwrap();
        assert_eq!(pos.size, quote!(150));
        assert_eq!(pos.entry_price, quote!(120));

        pos.increase_long(quote!(50), quote!(110)).unwrap();
        assert_eq!(pos.size, quote!(200));
        assert_eq!(pos.entry_price, quote!(117.5));

        // Make sure it does not work if a short position is set.
        pos.size = quote!(-250);
        assert_eq!(
            pos.increase_long(quote!(150), quote!(120)),
            Err(Error::OpenShort)
        );
    }

    #[test]
    fn decrease_long_position() {
        let mut pos = Position::default();
        pos.open_position(base!(1), quote!(150)).unwrap();
        assert!(pos.decrease_long(base!(1.1), quote!(150)).is_err());
        assert_eq!(
            pos.decrease_long(base!(0.5), quote!(160)).unwrap(),
            quote!(5)
        );
        assert_eq!(pos.entry_price, quote!(150));
        assert_eq!(pos.size, base!(0.5));

        // Make sure it does not work when a short is set
        pos.size = base!(-1);
        assert_eq!(
            pos.decrease_long(base!(0.5), quote!(100)),
            Err(Error::InvalidAmount)
        );
    }

    #[test]
    fn increase_short_position() {
        let mut pos = Position::default();
        pos.increase_short(base!(1), quote!(100)).unwrap();
        assert_eq!(pos.size, base!(-1));
        assert_eq!(pos.entry_price, quote!(100));

        // Make sure it does not work with a long posiion
        pos.size = base!(1);
        assert_eq!(
            pos.increase_short(base!(1), quote!(100)),
            Err(Error::OpenLong)
        );
    }

    #[test]
    fn decrease_short_position() {
        let mut pos = Position::default();
        assert_eq!(
            pos.decrease_short(base!(1), quote!(100)),
            Err(Error::InvalidAmount)
        );

        pos.open_position(base!(-1), quote!(100)).unwrap();
        assert_eq!(
            pos.decrease_short(base!(0.5), quote!(90)).unwrap(),
            quote!(5)
        );
    }
}
