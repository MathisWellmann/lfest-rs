use super::{CurrencyMarker, Mon, Monies};

/// Fee as a part per one hundred thousand.
/// The generic `MarkerTaker` marker indicates to the type system if its a maker or taker fee.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct Fee<MakerTaker> {
    /// A per cent mill or pcm is one one-thousandth of a percent.
    /// 2.5 basis points would be 25 pcm.
    per_cent_mille: i32,
    _fee_type: std::marker::PhantomData<MakerTaker>,
}

impl<MakerTaker> Fee<MakerTaker> {
    /// Create a new instance from a value denoted as a basis point (1 / 10_000)
    #[inline(always)]
    pub const fn from_basis_points(basis_points: i32) -> Self {
        Self {
            per_cent_mille: basis_points * 10,
            _fee_type: std::marker::PhantomData,
        }
    }

    /// Create a new instance from a value denoted as (1 / 100_000)
    #[inline(always)]
    pub const fn from_per_cent_mille(pcm: i32) -> Self {
        Self {
            per_cent_mille: pcm,
            _fee_type: std::marker::PhantomData,
        }
    }

    /// Compute the fraction of the `value` that is the fee.
    pub fn for_value<T, BaseOrQuote>(&self, value: Monies<T, BaseOrQuote>) -> Monies<T, BaseOrQuote>
    where
        T: Mon,
        BaseOrQuote: CurrencyMarker<T>,
    {
        Monies::new(*value.as_ref() * T::from(self.per_cent_mille) / T::from(100000))
    }
}

/// The fee limit orders pay.
#[derive(Debug, Clone, Copy)]
pub struct Maker;

/// The fee market orders pay.
#[derive(Debug, Clone, Copy)]
pub struct Taker;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_of_fee() {
        assert_eq!(std::mem::size_of::<Fee<Maker>>(), 4);
        assert_eq!(std::mem::size_of::<Fee<Taker>>(), 4);
    }
}
