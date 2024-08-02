use super::{Currency, QuoteCurrency};

/// Each Currency that is used as margin has to implement this trait.
/// The margin currency of an account defines which type of futures contract is
/// traded. Here is how the margin `Currency` maps to the futures type:
/// `QuoteCurrency`: linear futures.
/// `BaseCurrency`: inverse futures.
pub trait MarginCurrency: Currency {
    /// Compute the profit and loss.
    ///
    /// # Arguments:
    /// `entry_price`: The price at which the position was initiated.
    /// `exit_prie`: The price at which the position was exited.
    /// `quantity`: The amount of contracts traded. must be negative if short.
    ///
    /// # Arguments:
    /// Returns the profit and loss measured in the `PairedCurrency` of the size
    /// currency.
    fn pnl<S: Currency>(
        entry_price: QuoteCurrency,
        exit_price: QuoteCurrency,
        quantity: S,
    ) -> S::PairedCurrency;

    /// Compute the price paid for `self` (total_cost) for `quantity` number of contracts.
    fn price_paid_for_qty(&self, quantity: <Self as Currency>::PairedCurrency) -> QuoteCurrency;
}
