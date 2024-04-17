use crate::{
    account_tracker::statistical_moments,
    types::{Currency, LnReturns},
    Result,
};

/// Contains the cornish fisher outputs.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CornishFisherOutput<M> {
    pub var: f64,
    pub asset_value_at_risk: M,
}

/// Compute the Cornish-Fisher Value at Risk (CF-VaR)
///
/// # Arguments:
/// - 'log_returns': natural logarithmic return series: (p1 / p0).ln()
/// - `asset_value`: Serves as the base from which the `asset_value_at_risk` is computed.
/// - `confidence_interval`: in range [0.0, 1.0], usually something like 0.01 or
/// 0.05.
///
pub(crate) fn cornish_fisher_value_at_risk<C>(
    ln_returns: &LnReturns<'_, f64>,
    asset_value: C,
    confidence_interval: f64,
) -> Result<CornishFisherOutput<C>>
where
    C: Currency,
{
    let stats = statistical_moments(ln_returns.0);

    // Its a bit expensive to compute, so only warn if user opts-in
    #[cfg(feature = "cornish_fisher_domain_warning")]
    if stats.skew <= 6.0 * (2_f64.sqrt() - 1.0)
        && 27.0 * stats.excess_kurtosis
            - (216.0 + 66.0 * stats.skew.powi(2)) * stats.excess_kurtosis
            + 40.0 * stats.skew.powi(4)
            + 336.0 * stats.skew.powi(2)
            <= 0.0
    {
        // See <https://portfoliooptimizer.io/blog/corrected-cornish-fisher-expansion-improving-the-accuracy-of-modified-value-at-risk/>
        warn!("Cornish-Fisher expansion outside the domain of validity.");
    }

    let quantile = distrs::Normal::ppf(confidence_interval, 0.0, 1.0);

    let exp = quantile
        + (quantile.powi(2) - 1.0) * stats.skew / 6.0
        + (quantile.powi(3) - 3.0 * quantile) * stats.excess_kurtosis / 24.0
        - (2.0 * quantile.powi(3) - 5.0 * quantile) * stats.skew.powi(2) / 36.0;

    let var = stats.mean + stats.std_dev * exp;

    // If these were percent returns we'd use the commented out one.
    // But here we use ln returns, so we take the latter one.
    // let asset_value_at_risk = asset_value * C::new((1.0 + var).try_into()?);
    let asset_value_at_risk = asset_value * C::new(var.exp().try_into()?);

    Ok(CornishFisherOutput {
        var,
        asset_value_at_risk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        prelude::{Decimal, QuoteCurrency},
        quote,
        test_helpers::LN_RETS_H,
    };

    #[test]
    fn test_cornish_fisher_value_at_risk() {
        let _ = pretty_env_logger::try_init();
        // Comparison to the original implementation at <https://github.com/JDE65/D-ratio>.
        assert_eq!(
            cornish_fisher_value_at_risk(&LnReturns(&LN_RETS_H), quote!(1000.0), 0.05).unwrap(),
            CornishFisherOutput {
                var: -0.013637197569894961,
                asset_value_at_risk: quote!(986.455367753932277000),
            }
        );
    }
}
