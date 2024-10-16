use const_decimal::Decimal;

use super::Mon;

/// Interface for doing numeric operations and conversion from one constant decimal range to another in one go.
pub trait DecimalConversionOp<I, const D_SELF: u8, const D_RHS: u8> {
    /// Multiply `self` with a right hand side of a different constant decimal.
    fn mul_converting(&self, rhs: Decimal<I, D_RHS>) -> Decimal<I, D_SELF>;
}

impl<I, const DB: u8, const D_SELF: u8, const D_RHS: u8> DecimalConversionOp<I, D_SELF, D_RHS>
    for super::QuoteCurrency<I, DB, D_SELF>
where
    I: Mon<DB> + Mon<D_SELF>,
{
    fn mul_converting(&self, rhs: Decimal<I, D_RHS>) -> Decimal<I, D_SELF> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mul_converting() {
        // price of 100 * 2.5 basis points = 0.025 = 0.03 rounded up
        // assert_eq!(
        //     Decimal::<i32, 2>::try_from_scaled(100, 0)
        //         .unwrap()
        //         .mul_converting(Decimal::<i32, 5>::try_from_scaled(25, 1)),
        //     Decimal::<i32, 2>::try_from_scaled(3, 2).unwrap()
        // );
        todo!()
    }
}
