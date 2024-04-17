/// The statistical moments of a distribution
#[derive(Debug, Clone)]
pub struct StatisticalMoments {
    /// The sample mean of the values.
    pub mean: f64,
    /// The standard deviation of the values from the mean.
    pub std_dev: f64,
    /// The skew of the values.
    pub skew: f64,
    /// The kurtosis of the values.
    pub excess_kurtosis: f64,
}

/// Compute the four statistical moments: mean, std_dev, skew and kurtosis
pub fn statistical_moments(vals: &[f64]) -> StatisticalMoments {
    let mean = vals.iter().sum::<f64>() / vals.len() as f64;
    let n = vals.len() as f64;

    let variance = vals.iter().map(|v| (*v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();

    let skew =
        ((1.0 / n) * vals.iter().map(|v| (*v - mean).powi(3)).sum::<f64>()) / variance.powf(1.5);

    let kurtosis = ((1.0 / n) * vals.iter().map(|v| (*v - mean).powi(4)).sum::<f64>())
        / variance.powi(2)
        - 3.0;

    StatisticalMoments {
        mean,
        std_dev,
        skew,
        excess_kurtosis: kurtosis,
    }
}

#[cfg(test)]
mod tests {
    use rand::{thread_rng, Rng};
    use rand_distr::StandardNormal;

    use super::*;
    use crate::utils::tests::round;

    #[test]
    fn test_statistical_moments() {
        let mut rng = thread_rng();
        let vals = Vec::<f64>::from_iter((0..1_000_000).map(|_| rng.sample(StandardNormal)));

        let stats = statistical_moments(&vals);
        assert_eq!(round(stats.mean, 2), 0.0);
        assert_eq!(round(stats.std_dev, 2), 1.0);
        assert_eq!(round(stats.skew, 1), 0.0);
        assert_eq!(round(stats.excess_kurtosis, 1), 0.0);

        // From the example in scipy docs of the `skew` method.
        assert_eq!(
            statistical_moments(&[2.0, 8.0, 0.0, 4.0, 1.0, 9.0, 9.0, 0.0]).skew,
            0.2650554122698573
        );
        assert_eq!(
            statistical_moments(&[2.0, 8.0, 0.0, 4.0, 1.0, 9.0, 9.0, 0.0]).excess_kurtosis,
            -1.6660010752838508
        );
    }
}
