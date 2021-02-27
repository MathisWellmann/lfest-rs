#[derive(Debug, Clone)]
/// Welford online algorithm for estimating mean and variance
pub struct WelfordOnline {
    count: u64,
    mean: f64,
    s: f64,
}

impl WelfordOnline {
    /// Create a new WelfordOnline struct
    pub fn new() -> Self {
        WelfordOnline {
            count: 0,
            mean: 0.0,
            s: 0.0,
        }
    }

    /// Return the estimated variance
    pub fn variance(&self) -> f64 {
        if self.count > 1 {
            return self.s / (self.count - 1) as f64;
        }
        0.0
    }

    /// Return the standard deviation
    pub fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Add a new value to update the estimates
    pub fn add(&mut self, val: f64) {
        self.count += 1;
        let old_mean = self.mean;
        self.mean += (val - old_mean) / self.count as f64;
        self.s += (val - old_mean) * (val - self.mean);
    }
}
