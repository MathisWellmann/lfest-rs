/// When the limit order is priced at marketable prices (e.g a buy at or above the ask price),
/// decide what to do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RePricing {
    /// A limit order is marketable if it can take liquidity from the book.
    /// If the entry price locks or crosses the opposite top-of-book quote, it executes
    /// immediately at the best opposite quote (paying the taker fee) instead of being rejected.
    Marketable,
    /// If at the time of entry an order locks or crosses an away market quotation, the
    /// order will be immediately canceled back to the member.
    /// Good-Til-Crossing (GTX), sometimes referred to as limit maker or post-only orders,
    /// only add liquidity to the order book.
    /// On execution, if any part of a GTX limit order crosses the spread and matches a resting order,
    /// the entire GTX order is canceled without generating any fills.
    #[default]
    GoodTilCrossing,
}
