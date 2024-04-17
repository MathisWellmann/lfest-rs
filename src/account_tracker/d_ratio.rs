use crate::{
    cornish_fisher::cornish_fisher_value_at_risk,
    types::{Currency, LnReturns, MarginCurrency},
    Result,
};

/// Also called discriminant-ratio, which focuses on the added value of the
/// algorithm It uses the Cornish-Fish Value at Risk (CF-VaR)
/// It better captures the risk of the asset as it is not limited by the
/// assumption of a gaussian distribution It it time-insensitive
///
/// # Parameters:
/// - `returns_account`: The ln returns of the account.
/// - `returns_bnh`: The ln returns of buy and hold aka the market returns.
/// - `wallet_balance_start`: The starting margin balance of the account.
/// - `num_trading_days`: The number of trading days.
pub fn d_ratio<'a, M>(
    returns_account: LnReturns<'a, f64>,
    returns_bnh: LnReturns<'a, f64>,
    wallet_balance_start: M,
    num_trading_days: u64,
) -> Result<f64>
where
    M: Currency + MarginCurrency + Send,
{
    let cf_var_bnh = cornish_fisher_value_at_risk(&returns_bnh, wallet_balance_start, 0.01)?.var;
    let cf_var_acc =
        cornish_fisher_value_at_risk(&returns_account, wallet_balance_start, 0.01)?.var;

    let num_trading_days = num_trading_days as f64;

    // compute annualized returns
    let roi_acc = returns_account
        .0
        .iter()
        .fold(1.0, |acc, x| acc * x.exp())
        .powf(365.0 / num_trading_days);
    let roi_bnh = returns_bnh
        .0
        .iter()
        .fold(1.0, |acc, x| acc * x.exp())
        .powf(365.0 / num_trading_days);

    Ok((1.0 + (roi_acc - roi_bnh) / roi_bnh.abs()) * (cf_var_bnh / cf_var_acc))
}
