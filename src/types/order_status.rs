use getset::{CopyGetters, Getters};

use super::{order_meta::ExchangeOrderMeta, CurrencyMarker, Mon, QuoteCurrency, TimestampNs};

/// A new order has not been received by the exchange and has thus some pieces of information not available.
/// This also means the various filters (e.g `PriceFilter` and `QuantityFilter`) have not been checked.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewOrder;

/// The order is pending execution, but it already has additional information filled in by the exchange.
/// - `I`: The numeric data type of currencies.
/// - `DB`: The constant decimal precision of the `BaseCurrency`.
/// - `DQ`: The constant decimal precision of the `QuoteCurrency`.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
#[derive(Debug, Clone, Eq, PartialEq, Getters)]
pub struct Pending<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    /// The now filled in order metadata.
    #[getset(get = "pub")]
    meta: ExchangeOrderMeta,

    /// Information about the filled quantity.
    #[getset(get = "pub")]
    pub(crate) filled_quantity: FilledQuantity<I, DB, DQ, BaseOrQuote>,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> Pending<I, DB, DQ, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    /// Create a new instance of `Self`
    pub(crate) fn new(meta: ExchangeOrderMeta) -> Self {
        Self {
            meta,
            filled_quantity: FilledQuantity::Unfilled,
        }
    }
}

/// Contains the filled order quantity along with the average fill price.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FilledQuantity<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    /// All the order quantity has yet to be filled.
    Unfilled,
    /// Some (or all) of the order quantity has been filled.
    Filled {
        /// Cumulative Amount that was filled.
        cumulative_qty: BaseOrQuote,

        /// The average price it was filled at.
        avg_price: QuoteCurrency<I, DB, DQ>,
    },
}

/// The order has been fully filled.
/// The executed order quantity is stored elsewhere.
#[derive(Debug, Clone, Eq, PartialEq, Getters, CopyGetters)]
pub struct Filled<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    /// The now filled in order metadata.
    #[getset(get = "pub")]
    meta: ExchangeOrderMeta,

    /// The timestamp in nanoseconds when the order was executed by the exchange.
    /// Will be the simulated time, not actual computer (OS) time.
    #[getset(get_copy = "pub")]
    ts_ns_executed: TimestampNs,

    /// The average price the order has been filled at.
    #[getset(get_copy = "pub")]
    avg_fill_price: QuoteCurrency<I, DB, DQ>,

    /// The total filled quantity.
    #[getset(get_copy = "pub")]
    filled_qty: BaseOrQuote,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> Filled<I, DB, DQ, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    /// Create a new instance of `Self`.
    pub(crate) fn new(
        meta: ExchangeOrderMeta,
        ts_ns_executed: TimestampNs,
        avg_fill_price: QuoteCurrency<I, DB, DQ>,
        filled_qty: BaseOrQuote,
    ) -> Self {
        Self {
            meta,
            ts_ns_executed,
            avg_fill_price,
            filled_qty,
        }
    }
}
