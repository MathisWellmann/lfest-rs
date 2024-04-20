use getset::{CopyGetters, Getters};

use super::{order_meta::ExchangeOrderMeta, Currency, QuoteCurrency, TimestampNs};

/// A new order has not been received by the exchange and has thus some pieces of information not available.
/// This also means the various filters (e.g `PriceFilter` and `QuantityFilter`) have not been checked.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewOrder;

/// The order is pending execution, but it already has additional information filled in by the exchange.
#[derive(Debug, Clone, Eq, PartialEq, Getters)]
pub struct Pending<Q> {
    /// The now filled in order metadata.
    #[getset(get = "pub")]
    meta: ExchangeOrderMeta,

    /// Information about the filled quantity.
    #[getset(get = "pub")]
    pub(crate) filled_quantity: FilledQuantity<Q>,
}

impl<Q> Pending<Q>
where
    Q: Currency,
{
    /// Create a new instance of `Self`
    pub(crate) fn new(meta: ExchangeOrderMeta) -> Self {
        Self {
            meta,
            filled_quantity: FilledQuantity::Unfilled,
        }
    }

    /// Used when an order get some `quantity` filled at a `price`.
    pub(crate) fn fill(&mut self, price: QuoteCurrency, quantity: Q) {
        match &mut self.filled_quantity {
            FilledQuantity::Unfilled => {
                self.filled_quantity = FilledQuantity::Filled {
                    cumulative_qty: quantity,
                    avg_price: price,
                }
            }
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price,
            } => {
                let new_qty = *cumulative_qty + quantity;
                *avg_price = QuoteCurrency::new(
                    ((cumulative_qty.inner() * avg_price.inner())
                        + (price.inner() * quantity.inner()))
                        / new_qty.inner(),
                );
                *cumulative_qty = new_qty;
            }
        }
    }
}

/// Contains the filled order quantity along with the average fill price.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FilledQuantity<Q> {
    /// All the order quantity has yet to be filled.
    Unfilled,
    /// Some (or all) of the order quantity has been filled.
    Filled {
        /// Cumulative Amount that was filled.
        cumulative_qty: Q,

        /// The average price it was filled at.
        avg_price: QuoteCurrency,
    },
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

    /// The average price the order has been filled at.
    #[getset(get_copy = "pub")]
    avg_fill_price: QuoteCurrency,
}

impl Filled {
    /// Create a new instance of `Self`.
    pub(crate) fn new(
        meta: ExchangeOrderMeta,
        ts_ns_executed: TimestampNs,
        avg_fill_price: QuoteCurrency,
    ) -> Self {
        Self {
            meta,
            ts_ns_executed,
            avg_fill_price,
        }
    }
}
