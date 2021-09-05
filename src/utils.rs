/// Return the minimum of two values
#[inline]
pub(crate) fn min(v0: f64, v1: f64) -> f64 {
    if v0 < v1 {
        v0
    } else {
        v1
    }
}

/// Return the maximum of two values
#[inline]
pub(crate) fn max(v0: f64, v1: f64) -> f64 {
    if v0 > v1 {
        v0
    } else {
        v1
    }
}
