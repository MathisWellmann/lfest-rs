//! Protection collar bounding slippage on elected stop-market orders
//! (adapted from `OrderBook-rs` `stop_protection.rs`).

use num_traits::Zero;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use crate::types::{
    ConfigError,
    Mon,
    QuoteCurrency,
    Side,
};

/// Absolute price collar protecting elected stop-market orders against extreme
/// slippage through thin or gapped markets.
///
/// When a stop triggers at `stop_price`, its worst acceptable fill price is:
/// - `Buy` stop: `stop_price + collar`
/// - `Sell` stop: `stop_price - collar` (floored at zero)
///
/// If the market gaps past the protection limit, the elected stop cancels with
/// `StopCancelReason::StopProtectionBand` rather than executing at a ruinous fill.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StopProtection<I, const D: u8>
where
    I: Mon<D>,
{
    collar: QuoteCurrency<I, D>,
}

impl<I, const D: u8> StopProtection<I, D>
where
    I: Mon<D>,
{
    /// Create a validated [`StopProtection`] collar.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidStopProtection`] if `collar <= 0`.
    pub fn new(collar: QuoteCurrency<I, D>) -> Result<Self, ConfigError> {
        if collar <= QuoteCurrency::zero() {
            return Err(ConfigError::InvalidStopProtection);
        }
        Ok(Self { collar })
    }

    /// The configured collar offset in quote currency units.
    #[inline(always)]
    #[must_use]
    pub fn collar(&self) -> QuoteCurrency<I, D> {
        self.collar
    }

    /// Compute the worst acceptable execution price for an elected stop on `side`.
    #[inline]
    #[must_use]
    pub fn limit_price(&self, side: Side, stop_price: QuoteCurrency<I, D>) -> QuoteCurrency<I, D> {
        match side {
            Side::Buy => stop_price + self.collar,
            Side::Sell => {
                if self.collar >= stop_price {
                    QuoteCurrency::zero()
                } else {
                    stop_price - self.collar
                }
            }
        }
    }

    /// Return `true` if `execution_price` lies within the protection band.
    #[inline]
    #[must_use]
    pub fn allows_execution(
        &self,
        side: Side,
        stop_price: QuoteCurrency<I, D>,
        execution_price: QuoteCurrency<I, D>,
    ) -> bool {
        let limit = self.limit_price(side, stop_price);
        match side {
            Side::Buy => execution_price <= limit,
            Side::Sell => execution_price >= limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_protection_bounds_buy_and_sell_gaps() {
        assert_eq!(
            StopProtection::<i64, 2>::new(QuoteCurrency::zero()),
            Err(ConfigError::InvalidStopProtection)
        );
        let prot = StopProtection::<i64, 2>::new(QuoteCurrency::new(5, 0)).unwrap();
        // Sell stop at 100 -> band floor 95
        assert_eq!(
            prot.limit_price(Side::Sell, QuoteCurrency::new(100, 0)),
            QuoteCurrency::new(95, 0)
        );
        assert!(prot.allows_execution(
            Side::Sell,
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(95, 0)
        ));
        assert!(!prot.allows_execution(
            Side::Sell,
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(94, 0)
        ));
        // Buy stop at 100 -> band cap 105
        assert_eq!(
            prot.limit_price(Side::Buy, QuoteCurrency::new(100, 0)),
            QuoteCurrency::new(105, 0)
        );
        assert!(prot.allows_execution(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(105, 0)
        ));
        assert!(!prot.allows_execution(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(106, 0)
        ));
    }
}
