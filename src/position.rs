use fpdec::Decimal;
use getset::{CopyGetters, Getters};

use crate::types::{Currency, MarginCurrency, QuoteCurrency};

/// Describes the position information of the account.
/// It assumes isolated margining mechanism, because the margin is directly associated with the position.
#[derive(Debug, Clone, Default, Eq, PartialEq, Getters, CopyGetters)]
pub struct Position<M>
where
    M: Currency + MarginCurrency,
{
    /// The number of futures contracts making up the position.
    /// Denoted in the currency in which the size is valued.
    /// e.g.: XBTUSD has a contract size of 1 USD, so `M::PairedCurrency` is USD.
    #[getset(get_copy = "pub")]
    pub(crate) size: M::PairedCurrency,

    /// The entry price of the position
    #[getset(get_copy = "pub")]
    pub(crate) entry_price: QuoteCurrency,

    /// The position margin of account, same denotation as wallet_balance
    /// TODO: rename to `margin`.
    #[getset(get_copy = "pub")]
    pub(crate) position_margin: M,
}

impl<M> Position<M>
where
    M: Currency + MarginCurrency,
{
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
}
