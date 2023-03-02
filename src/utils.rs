use fpdec::Decimal;

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

/// Convert a `Decimal` value into `f64`.
#[inline(always)]
pub(crate) fn decimal_to_f64(val: Decimal) -> f64 {
    val.coefficient() as f64 / val.n_frac_digits() as f64
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rand::{thread_rng, Rng};

    use super::*;

    #[test]
    fn test_decimal_to_f64() {
        assert_eq!(decimal_to_f64(Decimal::ZERO), 0.0);
        assert_eq!(decimal_to_f64(Decimal::ONE), 1.0);
        assert_eq!(decimal_to_f64(Decimal::TWO), 2.0);

        let mut rng = thread_rng();
        for i in 0..1_000_000 {
            let val: f64 = rng.gen();
            // TODO: this may fail, whats a better way to test this?
            assert_eq!(decimal_to_f64(Decimal::try_from(val).unwrap()), val);
            assert_eq!(decimal_to_f64(Decimal::try_from(val * 10.0).unwrap()), val);
        }
    }
}
