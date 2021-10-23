/// Compute the four statistical moments: mean, std_dev, skew and kurtosis
fn statistical_moments(vals: &Vec<f64>) -> (f64, f64, f64, f64) {
    let mean = vals.iter().sum::<f64>() / vals.len() as f64;

    let variance = vals.iter().map(|v| (*v - mean).powi(2)).sum::<f64>() / vals.len() as f64;
    let std_dev = variance.sqrt();

    let skew = ((1.0 / vals.len() as f64) * vals.iter().map(|v| (*v - mean).powi(3)).sum::<f64>())
        / variance.powf(1.5);

    let kurtosis = ((1.0 / vals.len() as f64)
        * vals.iter().map(|v| (*v - mean).powi(4)).sum::<f64>())
        / variance.powi(2)
        - 3.0;

    (mean, std_dev, skew, kurtosis)
}

/// lookup the percentage point function
fn percent_point_function(q: f64) -> f64 {
    const SAMPLES: [f64; 100] = [
        f64::MIN,
        -2.3263478740408408,
        -2.053748910631823,
        -1.880793608151251,
        -1.75068607125217,
        -1.6448536269514729,
        -1.5547735945968535,
        -1.4757910281791706,
        -1.4050715603096329,
        -1.3407550336902165,
        -1.2815515655446004,
        -1.2265281200366098,
        -1.1749867920660904,
        -1.1263911290388007,
        -1.0803193408149558,
        -1.0364333894937898,
        -0.994457883209753,
        -0.9541652531461943,
        -0.915365087842814,
        -0.8778962950512288,
        -0.8416212335729142,
        -0.8064212470182404,
        -0.7721932141886848,
        -0.7388468491852137,
        -0.7063025628400874,
        -0.6744897501960817,
        -0.643345405392917,
        -0.6128129910166272,
        -0.5828415072712162,
        -0.5533847195556729,
        -0.5244005127080409,
        -0.4958503473474533,
        -0.46769879911450823,
        -0.4399131656732338,
        -0.41246312944140473,
        -0.38532046640756773,
        -0.3584587932511938,
        -0.33185334643681663,
        -0.3054807880993974,
        -0.27931903444745415,
        -0.2533471031357997,
        -0.22754497664114948,
        -0.20189347914185088,
        -0.17637416478086135,
        -0.15096921549677725,
        -0.12566134685507402,
        -0.10043372051146975,
        -0.0752698620998299,
        -0.05015358346473367,
        -0.02506890825871106,
        0.0,
        0.02506890825871106,
        0.05015358346473367,
        0.0752698620998299,
        0.10043372051146988,
        0.12566134685507416,
        0.1509692154967774,
        0.1763741647808612,
        0.20189347914185074,
        0.22754497664114934,
        0.2533471031357997,
        0.27931903444745415,
        0.3054807880993974,
        0.33185334643681663,
        0.3584587932511938,
        0.38532046640756773,
        0.41246312944140495,
        0.4399131656732339,
        0.4676987991145084,
        0.4958503473474532,
        0.5244005127080407,
        0.5533847195556727,
        0.5828415072712162,
        0.6128129910166272,
        0.643345405392917,
        0.6744897501960817,
        0.7063025628400874,
        0.7388468491852137,
        0.7721932141886848,
        0.8064212470182404,
        0.8416212335729143,
        0.8778962950512289,
        0.9153650878428138,
        0.9541652531461943,
        0.994457883209753,
        1.0364333894937898,
        1.0803193408149558,
        1.1263911290388007,
        1.1749867920660904,
        1.2265281200366105,
        1.2815515655446004,
        1.3407550336902165,
        1.4050715603096329,
        1.475791028179171,
        1.5547735945968535,
        1.6448536269514722,
        1.7506860712521692,
        1.8807936081512509,
        2.0537489106318225,
        2.3263478740408408,
    ];
    SAMPLES[(q * 100.0) as usize]
}

/// Compute the Cornish-Fisher Value at Risk (CF-VaR)
/// # Arguments:
/// log_returns: logarithmic return series: (p1 / p0).ln()
/// asset_value: current asset value
/// confidence_interval: in range [0.0, 1.0], usually something like 0.01 or 0.05
/// # Returns:
/// tuple containing (cf_exp, cf_var, cf_asset_value)
/// of most importance is cf_var which if the actual CF-VaR
pub(crate) fn cornish_fisher_value_at_risk(
    log_returns: &Vec<f64>,
    asset_value: f64,
    confidence_interval: f64,
) -> (f64, f64, f64) {
    let (mean, std_dev, skew, kurtosis) = statistical_moments(log_returns);

    let quantile = percent_point_function(confidence_interval);
    let cf_exp = quantile
        + (quantile.powi(2) - 1.0) * skew / 6.0
        + (quantile.powi(3) - 3.0 * quantile) * kurtosis / 24.0
        - (2.0 * quantile.powi(3) - 5.0 * quantile) * skew.powi(2) / 36.0;
    let cf_var = mean + std_dev * cf_exp;
    //let cf_asset_value = asset_value * (1.0 + cf_var); // like in the paper, but wrong as the underlying returns are logarithmic
    let cf_asset_value = asset_value - (asset_value * cf_var.exp());

    (cf_exp, cf_var, cf_asset_value)
}

#[cfg(test)]
mod tests {
    use rand::{thread_rng, Rng};
    use rand_distr::StandardNormal;

    use super::*;
    use crate::round;

    #[test]
    fn test_statistical_moments() {
        let mut rng = thread_rng();
        let vals: Vec<f64> = (0..10_000).map(|_| rng.sample(StandardNormal)).collect();

        let (mean, std_dev, skew, kurtosis) = statistical_moments(&vals);
        assert_eq!(round(mean, 1), 0.0);
        assert_eq!(round(std_dev, 1), 1.0);
        assert_eq!(round(skew, 0), 0.0);
        assert_eq!(round(kurtosis, 0), 0.0);
    }

    #[test]
    fn test_percentage_point_function() {
        assert_eq!(round(percent_point_function(0.01), 2), -2.33);
        assert_eq!(round(percent_point_function(0.05), 2), -1.64);
        assert_eq!(round(percent_point_function(0.90), 2), 1.28);
        assert_eq!(round(percent_point_function(0.95), 2), 1.64);
        assert_eq!(round(percent_point_function(0.99), 2), 2.33);
    }
}
