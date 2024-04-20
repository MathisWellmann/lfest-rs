use getset::CopyGetters;

use super::OrderId;

/// Additional data about the order filled in by the exchange.
#[derive(Debug, Clone, PartialEq, Eq, CopyGetters)]
pub struct ExchangeOrderMeta {
    /// The global order sequence number assigned by the exchange upon receiving it.
    #[getset(get_copy = "pub")]
    id: OrderId,
    /// timestamp in nanoseconds, when the exchange has received the order.
    /// Will be the simulated time, not actual computer (OS) time.
    #[getset(get_copy = "pub")]
    ts_ns_exchange_received: i64,
}

impl ExchangeOrderMeta {
    /// Create a new instance of `Self`.
    pub(crate) fn new(id: OrderId, ts_ns_exchange_received: i64) -> Self {
        Self {
            id,
            ts_ns_exchange_received,
        }
    }
}
