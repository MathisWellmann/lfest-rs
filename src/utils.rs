use assert2::assert;
use const_decimal::Decimal;

use crate::prelude::*;

/// When no user specified order id is required.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoUserOrderId;

impl std::fmt::Display for NoUserOrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}

/// Return the minimum of two values
#[inline(always)]
pub(crate) fn min<T>(v0: T, v1: T) -> T
where
    T: PartialOrd,
{
    if v0 < v1 {
        v0
    } else {
        v1
    }
}

/// Return the maximum of two values
#[inline(always)]
pub(crate) fn max<T>(v0: T, v1: T) -> T
where
    T: PartialOrd,
{
    if v0 > v1 {
        v0
    } else {
        v1
    }
}

/// Asserts that the users wallet balance is greater than zero.
#[inline]
pub(crate) fn assert_user_wallet_balance<I, const D: u8, Acc, BaseOrQuote>(
    transaction_accounting: &Acc,
) where
    I: Mon<D>,
    Acc: TransactionAccounting<I, D, BaseOrQuote>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    let wallet_balance = transaction_accounting
        .margin_balance_of(USER_WALLET_ACCOUNT)
        .expect("is valid");
    assert!(wallet_balance >= BaseOrQuote::zero());
}

/// Create a `Decimal` from an `f64` value.
// TODO: maybe upstream this impl to `const_decimal`
#[inline(always)]
pub fn decimal_from_f64<I: Mon<D>, const D: u8>(val: f64) -> Result<Decimal<I, D>> {
    let scaling_factor = 10_f64.powi(D as i32);
    let scaled: f64 = (val * scaling_factor).round();
    Ok(
        Decimal::try_from_scaled(I::from(scaled as i64).ok_or(Error::IntegerConversion)?, D)
            .ok_or(Error::UnableToCreateDecimal)?,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use const_decimal::Decimal;

    use crate::utils::decimal_from_f64;

    #[test]
    fn test_convert_decimals() {
        assert_eq!(
            Decimal::<i32, 0>::try_from_scaled(10, 1)
                .expect("can convert")
                .0,
            1
        );
    }

    #[test]
    fn test_decimal_from_f64() {
        assert_eq!(
            decimal_from_f64(3.0).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(3, 0).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.1).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(31, 1).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.14).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(314, 2).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.141).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(3141, 3).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.1415).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(31415, 4).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.14159).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(314159, 5).unwrap()
        );
    }
}
