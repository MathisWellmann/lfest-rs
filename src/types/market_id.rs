//! Dense market identifier and lifecycle status for multi-symbol portfolios.

use std::fmt::Display;

/// Dense zero-based index identifying a market inside a [`crate::portfolio::PortfolioExchange`].
///
/// Using a compact `u16` newtype avoids hashing on the hot path and prevents
/// mixing market indices with order IDs or timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct MarketId(u16);

impl MarketId {
    /// Create a [`MarketId`] from a raw `u16` index.
    #[inline(always)]
    #[must_use]
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Return the underlying `u16` index.
    #[inline(always)]
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Convert to `usize` for slice indexing.
    #[inline(always)]
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl From<u16> for MarketId {
    #[inline(always)]
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MarketId({})", self.0)
    }
}

/// Lifecycle state of a market in a multi-symbol exchange.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarketStatus {
    /// Normal trading: both risk-increasing and risk-reducing orders are accepted.
    #[default]
    Active,
    /// Only strictly position-reducing orders and cancellations are accepted;
    /// new risk-increasing orders are rejected.
    ReduceOnly,
    /// Market is delisted: all resting orders have been cancelled and any open
    /// position has been force-settled at the settlement price. No further orders
    /// are accepted.
    Delisted,
}
