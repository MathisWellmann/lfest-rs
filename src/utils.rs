/// Return the minimum of two values
#[inline(always)]
pub(crate) fn min<T>(v0: T, v1: T) -> T
where T: PartialOrd {
    if v0 < v1 {
        v0
    } else {
        v1
    }
}

/// Return the maximum of two values
#[inline(always)]
pub(crate) fn max<T>(v0: T, v1: T) -> T
where T: PartialOrd {
    if v0 > v1 {
        v0
    } else {
        v1
    }
}

/// round a value to a given precision of decimal places
/// used in tests
#[inline(always)]
#[cfg(test)]
pub fn round(val: f64, prec: i32) -> f64 {
    ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round() {
        assert_eq!(round(0.111111, 0), 0.0);
        assert_eq!(round(0.111111, 1), 0.1);
        assert_eq!(round(0.111111, 2), 0.11);
        assert_eq!(round(0.111111, 3), 0.111);
        assert_eq!(round(0.111111, 4), 0.1111);
        assert_eq!(round(0.111111, 5), 0.11111);
        assert_eq!(round(0.111111, 6), 0.111111);
    }
}
