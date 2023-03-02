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

/// round a value to a given precision of decimal places
/// used in tests
#[inline(always)]
pub fn round(val: f64, prec: i32) -> f64 {
    ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
}

/// Convert the minimum (price, size) increment to decimal places
#[inline(always)]
#[deprecated]
pub(crate) fn decimal_places_from_min_incr(min_incr: f64) -> i32 {
    (1.0 / min_incr).log10().ceil() as i32
}

/// TODO: create PR to `impl std::iter::Sum for Decimal` on upstream `fpdec`
/// Sums a bunch of decimals up
pub(crate) fn sum_decimals(vals: &[Decimal]) -> Decimal {
    let mut out = Decimal::ZERO;
    for v in vals {
        out += v;
    }
    out
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
    fn test_round() {
        assert_eq!(round(0.111111, 0), 0.0);
        assert_eq!(round(0.111111, 1), 0.1);
        assert_eq!(round(0.111111, 2), 0.11);
        assert_eq!(round(0.111111, 3), 0.111);
        assert_eq!(round(0.111111, 4), 0.1111);
        assert_eq!(round(0.111111, 5), 0.11111);
        assert_eq!(round(0.111111, 6), 0.111111);
    }

    #[test]
    fn test_decimal_places_from_min_incr() {
        assert_eq!(decimal_places_from_min_incr(5.0), 0);
        assert_eq!(decimal_places_from_min_incr(0.3), 1);
        assert_eq!(decimal_places_from_min_incr(0.7), 1);
        assert_eq!(decimal_places_from_min_incr(0.03), 2);
        assert_eq!(decimal_places_from_min_incr(0.07), 2);
        assert_eq!(decimal_places_from_min_incr(0.003), 3);
        assert_eq!(decimal_places_from_min_incr(0.007), 3);
    }

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
