use super::{Currency, Mon, QuoteCurrency};

/// Each Currency that is used as margin has to implement this trait.
/// The margin currency of an account defines which type of futures contract is
/// traded. Here is how the margin `Currency` maps to the futures type:
/// `QuoteCurrency`: linear futures.
/// `BaseCurrency`: inverse futures.
///
/// # Generics:
/// - `I` is the numeric type,
/// - `D` is the constant decimal precision.
pub trait MarginCurrency<I, const D: u8>: Currency<I, D>
where
    I: Mon<D>,
{
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
    ///
    fn pnl(
        entry_price: QuoteCurrency<I, D>,
        exit_price: QuoteCurrency<I, D>,
        quantity: Self::PairedCurrency,
    ) -> Self;
}
