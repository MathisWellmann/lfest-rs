use getset::{CopyGetters, Getters};

use super::{order_meta::ExchangeOrderMeta, TimestampNs};

/// A new order has not been received by the exchange and has thus some pieces of information not available.
/// This also means the various filters (e.g `PriceFilter` and `QuantityFilter`) have not been checked.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewOrder;

/// The order is pending execution, but it already has additional information filled in by the exchange.
#[derive(Debug, Clone, Eq, PartialEq, Getters)]
pub struct Pending {
    /// The now filled in order metadata.
    #[getset(get = "pub")]
    meta: ExchangeOrderMeta,
}

impl Pending {
    /// Create a new instance of `Self`
    pub(crate) fn new(meta: ExchangeOrderMeta) -> Self {
        Self { meta }
    }
}

/// The order has been fully filled.
/// The executed order quantity is stored elsewhere.
#[derive(Debug, Clone, Eq, PartialEq, Getters, CopyGetters)]
pub struct Filled {
    /// The now filled in order metadata.
    #[getset(get = "pub")]
    meta: ExchangeOrderMeta,
    /// The timestamp in nanoseconds when the order was executed by the exchange.
    /// Will be the simulated time, not actual computer (OS) time.
    #[getset(get_copy = "pub")]
    ts_ns_executed: TimestampNs,
}

impl Filled {
    /// Create a new instance of `Self`.
    pub(crate) fn new(meta: ExchangeOrderMeta, ts_ns_executed: TimestampNs) -> Self {
        Self {
            meta,
            ts_ns_executed,
        }
    }
}
