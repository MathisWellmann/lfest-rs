use assert2::assert;

use crate::{
    prelude::{TransactionAccounting, UserBalances, USER_WALLET_ACCOUNT},
    types::MarginCurrency,
};

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
pub(crate) fn assert_user_wallet_balance<T, M>(transaction_accounting: &T)
where
    T: TransactionAccounting<M>,
    M: MarginCurrency,
{
    let wallet_balance = transaction_accounting
        .margin_balance_of(USER_WALLET_ACCOUNT)
        .expect("is valid");
    assert!(wallet_balance >= M::new_zero());
}

/// Sum of all balances in users `TAccount`s.
pub(crate) fn balance_sum<M: MarginCurrency>(user_balances: &UserBalances<M>) -> M {
    user_balances.available_wallet_balance
        + user_balances.position_margin
        + user_balances.order_margin
}

#[cfg(test)]
pub(crate) mod tests {
    /// round a value to a given precision of decimal places
    /// used in tests
    pub(crate) fn round(val: f64, prec: i32) -> f64 {
        ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
    }
}
