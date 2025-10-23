use const_decimal::Decimal;

use super::Mon;

/// Fee as a part per one hundred thousand.
/// The generic `MarkerTaker` marker indicates to the type system if its a maker or taker fee.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fee<I, const D: u8, MakerTaker> {
    value: Decimal<I, D>,
    _fee_type: std::marker::PhantomData<MakerTaker>,
}

impl<I, const D: u8, MakerTaker> From<Decimal<I, D>> for Fee<I, D, MakerTaker>
where
    I: Mon<D>,
{
    fn from(value: Decimal<I, D>) -> Self {
        Self {
            value,
            _fee_type: std::marker::PhantomData,
        }
    }
}

impl<I, const D: u8, MakerTaker> AsRef<Decimal<I, D>> for Fee<I, D, MakerTaker> {
    #[inline]
    fn as_ref(&self) -> &Decimal<I, D> {
        &self.value
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
        assert_eq!(size_of::<Fee<i32, 5, Maker>>(), 4);
        assert_eq!(size_of::<Fee<i32, 5, Taker>>(), 4);
        assert_eq!(size_of::<Fee<i64, 5, Maker>>(), 8);
    }
}
