use getset::CopyGetters;

use super::{OrderId, TimestampNs};

/// Additional data about the order filled in by the exchange.
#[derive(Debug, Clone, PartialEq, Eq, CopyGetters)]
#[cfg_attr(test, derive(Default))]
pub struct ExchangeOrderMeta {
    /// The global order sequence number assigned by the exchange upon receiving it.
    #[getset(get_copy = "pub")]
    id: OrderId,
    /// timestamp in nanoseconds, when the exchange has received the order.
    /// Will be the simulated time, not actual computer (OS) time.
    #[getset(get_copy = "pub")]
    ts_exchange_received: TimestampNs,
}

impl ExchangeOrderMeta {
    /// Create a new instance of `Self`.
    pub fn new(id: OrderId, ts_ns_exchange_received: TimestampNs) -> Self {
        Self {
            id,
            ts_exchange_received: ts_ns_exchange_received,
        }
    }
}

impl std::fmt::Display for ExchangeOrderMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ExchangeOrderMeta( id: {}, ts_ns_exchange_received: {})",
            self.id, self.ts_exchange_received
        )
    }
}
