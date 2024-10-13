use super::{CurrencyMarker, Mon, QuoteCurrency};

/// Each Currency that is used as margin has to implement this trait.
/// The margin currency of an account defines which type of futures contract is
/// traded. Here is how the margin `Currency` maps to the futures type:
/// `QuoteCurrency`: linear futures.
/// `BaseCurrency`: inverse futures.
///
/// # Generics:
/// - `I` is the numeric type,
/// - `DB` is the constant decimal precision of the `BaseCurrency`.
/// - `DQ` is the constant decimal precision of the `QuoteCurrency`.
pub trait MarginCurrencyMarker<I, const DB: u8, const DQ: u8>: CurrencyMarker<I, DB, DQ>
where
    I: Mon<DQ> + Mon<DB>,
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
        entry_price: QuoteCurrency<I, DB, DQ>,
        exit_price: QuoteCurrency<I, DB, DQ>,
        quantity: Self::PairedCurrency,
    ) -> Self;

    /// Compute the price paid for the `total_cost` for `quantity` number of contracts.
    fn price_paid_for_qty(
        total_cost: Self,
        quantity: Self::PairedCurrency,
    ) -> QuoteCurrency<I, DB, DQ>;
}
