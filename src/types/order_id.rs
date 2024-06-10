use std::fmt::Display;

/// The type for the global order id sequence number used by the exchange.
#[derive(Debug, Default, Clone, Copy, std::hash::Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderId(u64);

impl From<u64> for OrderId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl OrderId {
    /// Increment the order id by one.
    pub(crate) fn incr(&mut self) {
        self.0 += 1
    }
}

impl Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
