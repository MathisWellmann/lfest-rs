use fpdec::Decimal;

use crate::{
    prelude::{TransactionAccounting, UserBalances, USER_WALLET_ACCOUNT},
    types::MarginCurrency,
};

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

/// Get the number of decimal places from a given step size
/// e.G step_size 0.001 -> 3
///     step_size 0.1 > 1
#[cfg(test)]
pub fn get_num_digits(step_size: Decimal) -> i32 {
    if step_size == fpdec::Dec!(0) {
        return 0;
    }
    let n = Decimal::ONE / step_size;
    decimal_to_f64(n).log10().ceil() as i32
}

/// Convert a `Decimal` value into `f64`.
/// Just used when accuracy is not required.
/// Mainly for `FullTrack` to compute things that are not supported by `Decimal`
/// such as sqrt.
pub(crate) fn decimal_to_f64(val: Decimal) -> f64 {
    if val.n_frac_digits() == 0 {
        return val.coefficient() as f64;
    }

    val.coefficient() as f64 / 10_f64.powi(val.n_frac_digits() as _)
}

/// Convert a `f64` value to a `Decimal` type given the `step_size` of the
/// `PriceFilter`
#[cfg(test)]
pub fn f64_to_decimal(price: f64, step_size: Decimal) -> Decimal {
    let decimal_places = get_num_digits(step_size);
    if decimal_places == 0 {
        return Decimal::from(price as i64);
    }
    let scaling = 10_f64.powi(decimal_places);
    let scaled_f64 = price * scaling;

    if decimal_places > 0 {
        Decimal::from(scaled_f64 as i64) / Decimal::from(10_i32.pow(decimal_places as u32))
    } else {
        Decimal::from(scaled_f64 as i64)
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
    tracing::trace!("wallet_balance: {wallet_balance}");
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
