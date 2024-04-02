use fpdec::Decimal;

use crate::{
    quote,
    types::{Currency, Leverage, MarginCurrency, QuoteCurrency},
};

#[derive(Debug, Clone, Default, Eq, PartialEq)]
/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
pub struct Position<M>
where
    M: Currency + MarginCurrency,
{
    /// The number of futures contracts making up the position.
    /// Denoted in the currency in which the size is valued.
    /// e.g.: XBTUSD has a contract size of 1 USD, so `M::PairedCurrency` is USD.
    pub(crate) size: M::PairedCurrency,
    /// The entry price of the position
    pub(crate) entry_price: QuoteCurrency,
    /// The position margin of account, same denotation as wallet_balance
    pub(crate) position_margin: M,
    /// The position leverage,
    pub(crate) leverage: Leverage,
}

impl<M> Position<M>
where
    M: Currency + MarginCurrency,
{
    pub(crate) fn new(leverage: Leverage) -> Self {
        Self {
            leverage,
            ..Default::default()
        }
    }

    /// Return the position size
    #[inline(always)]
    pub fn size(&self) -> M::PairedCurrency {
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

    /// Return the set leverage of the position
    #[inline(always)]
    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    /// Returns the implied leverage of the position based on the position value and the collateral backing it.
    /// It is computed by dividing the total value of the position by the amount of margin required to hold that position.
    #[inline]
    pub fn implied_leverage(&self, price: QuoteCurrency) -> Decimal {
        let value = self.size.convert(price);
        value.inner() / self.position_margin.inner()
    }

    /// Return the positions unrealized profit and loss
    /// denoted in QUOTE when using linear futures,
    /// denoted in BASE when using inverse futures
    pub fn unrealized_pnl(&self, bid: QuoteCurrency, ask: QuoteCurrency) -> M {
        // The upnl is based on the possible fill price, not the mid-price, which is more conservative
        if self.size > M::PairedCurrency::new_zero() {
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
    ///
    pub(crate) fn open_position(&mut self, size: M::PairedCurrency, price: QuoteCurrency) {
        debug_assert!(price > quote!(0));

        self.size = size;
        self.entry_price = price;
        self.position_margin = self.size.abs().convert(self.entry_price) / self.leverage;
    }

    /// Increase a long (or neutral) position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The price at which it is sold.
    ///
    pub(crate) fn increase_long(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "`amount` must be positive"
        );
        debug_assert!(self.size >= M::PairedCurrency::new_zero(), "Short is open");

        let new_size = self.size + quantity;
        self.entry_price = QuoteCurrency::new(
            (self.entry_price * self.size.inner() + price * quantity.inner()).inner()
                / new_size.inner(),
        );

        self.size = new_size;
        self.position_margin = self.size.abs().convert(self.entry_price) / self.leverage;
    }

    /// Reduce a long position.
    ///
    /// # Arguments:
    /// `amount`: The amount to decrease the position by, must be smaller or equal to the position size.
    /// `price`: The price at which it is sold.
    ///
    /// # Returns:
    /// If Ok, the net realized profit and loss for that specific futures contract.
    #[must_use]
    pub(crate) fn decrease_long(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) -> M {
        debug_assert!(
            self.size > M::PairedCurrency::new_zero(),
            "Open short or no position"
        );
        debug_assert!(quantity > M::PairedCurrency::new_zero());
        debug_assert!(quantity <= self.size, "Quantity larger than position size");

        self.size -= quantity;
        self.position_margin = self.size.abs().convert(self.entry_price) / self.leverage;

        M::pnl(self.entry_price, price, quantity)
    }

    /// Increase a short position.
    ///
    /// # Arguments:
    /// `amount`: The absolute amount to increase the short position by.
    ///     The `amount` must have been approved by the `RiskEngine`.
    /// `price`: The entry price.
    ///
    pub(crate) fn increase_short(&mut self, quantity: M::PairedCurrency, price: QuoteCurrency) {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "Amount must be positive; qed"
        );
        debug_assert!(
            self.size <= M::PairedCurrency::new_zero(),
            "Position must not be long; qed"
        );

        let new_size = self.size - quantity;
        self.entry_price = QuoteCurrency::new(
            (self.entry_price.inner() * self.size.inner().abs() + price.inner() * quantity.inner())
                / new_size.inner().abs(),
        );
        self.size = new_size;
        self.position_margin = self.size.abs().convert(self.entry_price) / self.leverage;
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
    pub(crate) fn decrease_short(
        &mut self,
        quantity: M::PairedCurrency,
        price: QuoteCurrency,
    ) -> M {
        debug_assert!(
            quantity > M::PairedCurrency::new_zero(),
            "Amount must be positive; qed"
        );
        debug_assert!(
            self.size < M::PairedCurrency::new_zero(),
            "Position must be short!"
        );
        debug_assert!(
            quantity <= self.size.abs(),
            "Amount must be smaller than short position; qed"
        );

        self.size += quantity;
        self.position_margin = self.size.abs().convert(self.entry_price) / self.leverage;

        M::pnl(self.entry_price, price, quantity.into_negative())
    }
}
