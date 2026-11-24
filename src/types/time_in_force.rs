//! Time-in-force and execution flags (`ReduceOnly`) for market and limit orders.

/// Time-in-force policy governing how a limit order behaves upon admission.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    /// Post-only / Good-Til-Crossing (default for backwards compatibility):
    /// rejects immediately if the limit price locks or crosses the best opposite quote.
    #[default]
    GoodTilCrossing,
    /// Good-Til-Cancelled: executes immediately at best opposite quote if marketable
    /// (paying taker fee), otherwise rests on the book until filled or cancelled.
    GoodTilCancelled,
    /// Immediate-Or-Cancel: executes immediately if marketable, cancelling any unfilled
    /// remainder rather than resting on the book.
    ImmediateOrCancel,
    /// Fill-Or-Kill: executes in full immediately if marketable, otherwise cancelled
    /// untouched without resting.
    FillOrKill,
}
