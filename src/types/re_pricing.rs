/// When the limit order is priced at marketable prices (e.g a buy at or above the ask price),
/// decide what to do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RePricing {
    // TODO: impl `Marketable`, requiring proper order book and matching engine i suppose.
    // A limit order is marketable if it can take liquidity from the book.
    // if the entry price locks or crosses an away market quotation it will immediately be filled
    // and the remaining quantity will rest in the book afterwards, if any.
    // Marketable,
    /// If at the time of entry an order locks or crosses an away market quotation, the
    /// order will be immediately canceled back to the member.
    /// Good-Til-Crossing (GTX), sometimes referred to as limit maker or post-only orders,
    /// only add liquidity to the order book.
    /// On execution, if any part of a GTX limit order crosses the spread and matches a resting order,
    /// the entire GTX order is canceled without generating any fills.
    #[default]
    GoodTilCrossing,
    // TODO: other variants:
    // If at the time of entry an order locks or crosses an away market quotation, the
    // order will be displayed and ranked one tick away from the locking price.
    // PriceAdjust,
    // Instead of sliding the limit price, hide the order until the away market quotation is lifted and the limit
    // order can be placed passively into the book at the original `limit_price` level.
    // HideNotSlide,
}
