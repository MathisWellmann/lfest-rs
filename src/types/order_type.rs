use super::{QuoteCurrency, Side};

/// Defines the available order types
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OrderType {
    /// taker market order.
    Market {
        /// Wether its a buy or a sell order.
        side: Side,
    },
    /// passive limit order.
    Limit {
        /// Wether its a buy or a sell order.
        side: Side,
        /// The price limit at which to place the order into the book.
        limit_price: QuoteCurrency,
    },
}
