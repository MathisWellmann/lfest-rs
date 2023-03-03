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
    if val.n_frac_digits() == 0 {
        return val.coefficient() as f64;
    }
    info!("val: {}, coeff: {}, n_frac_digits: {}", val, val.coefficient(), val.n_frac_digits());

    val.coefficient() as f64 / 10_f64.powi(val.n_frac_digits() as _) as f64
}

#[cfg(test)]
pub(crate) mod tests {
    use std::convert::TryFrom;

    use rand::{thread_rng, Rng};

    use super::*;

    /// round a value to a given precision of decimal places
    /// used in tests
    pub(crate) fn round(val: f64, prec: i32) -> f64 {
        ((val * 10.0_f64.powi(prec)).round()) / 10.0_f64.powi(prec)
    }

    #[test]
    fn test_decimal_to_f64() {
        let _ = pretty_env_logger::try_init();

        assert_eq!(decimal_to_f64(Decimal::ZERO), 0.0);
        assert_eq!(decimal_to_f64(Decimal::ONE), 1.0);
        assert_eq!(decimal_to_f64(Decimal::TWO), 2.0);

        let mut rng = thread_rng();
        const ROUNDING: i32 = 10;
        for _i in 0..1_000 {
            let val: f64 = rng.gen();
            assert_eq!(
                round(decimal_to_f64(Decimal::try_from(val).unwrap()), ROUNDING),
                round(val, ROUNDING)
            );
            assert_eq!(
                round(decimal_to_f64(Decimal::try_from(val * 10.0).unwrap()), ROUNDING),
                round(val * 10.0, ROUNDING)
            );
            assert_eq!(
                round(decimal_to_f64(Decimal::try_from(val * 100.0).unwrap()), ROUNDING),
                round(val * 100.0, ROUNDING)
            );
        }
    }
}
