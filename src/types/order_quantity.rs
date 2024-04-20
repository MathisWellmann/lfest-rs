use getset::{CopyGetters, Getters};

use super::{Currency, QuoteCurrency};

/// Information about the total order quantity along with how much was filled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Getters, CopyGetters)]
pub struct OrderQuantity<Q>
where
    Q: Currency,
{
    /// The total order quantity, denoted in base or quote currency `S`.
    #[getset(get_copy = "pub")]
    total: Q,

    /// Fill information.
    #[getset(get = "pub")]
    filled: FilledQuantity<Q>,
}

impl<Q> OrderQuantity<Q>
where
    Q: Currency,
{
    /// Create a new instance where all the quantity is unfilled.
    pub(crate) fn new_unfilled(qty: Q) -> Self {
        Self {
            total: qty,
            filled: FilledQuantity::Unfilled,
        }
    }

    /// Marks the order quantity as all filled. Does not handle fractional order fills.
    pub(crate) fn fill(&mut self, fill_price: QuoteCurrency) {
        self.filled = FilledQuantity::Filled {
            cumulative_qty: self.total,
            avg_price: fill_price,
        }
    }
}

/// Contains the filled order quantity along with the average fill price.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FilledQuantity<S> {
    /// All the order quantity has yet to be filled.
    Unfilled,
    /// Some (or all) of the order quantity has been filled.
    Filled {
        /// Cumulative Amount that was filled.
        cumulative_qty: S,

        /// The average price it was filled at.
        avg_price: QuoteCurrency,
    },
}
