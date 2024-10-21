use assert2::assert;

use crate::prelude::*;

/// Return the minimum of two values
#[inline]
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
#[inline]
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

/// Sum of all balances in users `TAccount`s.
pub(crate) fn balance_sum<I, const D: u8, BaseOrQuote>(
    user_balances: &UserBalances<BaseOrQuote>,
) -> BaseOrQuote
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    user_balances.available_wallet_balance
        + user_balances.position_margin
        + user_balances.order_margin
}

#[cfg(test)]
pub(crate) mod tests {
    use const_decimal::Decimal;

    /// round a value to a given precision of decimal places
    /// used in tests
    pub(crate) fn round(val: f64, prec: i32) -> f64 {
        ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
    }

    #[test]
    fn test_convert_decimals() {
        assert_eq!(
            Decimal::<i32, 0>::try_from_scaled(10, 1)
                .expect("can convert")
                .0,
            1
        );
    }
}
