use fpdec::{Dec, Decimal};

/// Return the minimum of two values
#[inline(always)]
pub(crate) fn min<T>(v0: T, v1: T) -> T
where
    T: PartialOrd,
{
    if v0 < v1 {
        v0
    } else {
        v1
    }
}

/// Return the maximum of two values
#[inline(always)]
pub(crate) fn max<T>(v0: T, v1: T) -> T
where
    T: PartialOrd,
{
    if v0 > v1 {
        v0
    } else {
        v1
    }
}

/// Get the number of decimal places from a given step size
/// e.G step_size 0.001 -> 3
///     step_size 0.1 > 1
pub fn get_num_digits(step_size: Decimal) -> i32 {
    if step_size == Dec!(0) {
        return 0;
    }
    let n = Decimal::ONE / step_size;
    decimal_to_f64(n).log10().ceil() as i32
}

/// Convert a `Decimal` value into `f64`.
/// Just used when accuracy is not required.
/// Mainly for `FullTrack` to compute things that are not supported by `Decimal`
/// such as sqrt.
pub(crate) fn decimal_to_f64(val: Decimal) -> f64 {
    if val.n_frac_digits() == 0 {
        return val.coefficient() as f64;
    }

    val.coefficient() as f64 / 10_f64.powi(val.n_frac_digits() as _)
}

/// Convert a `f64` value to a `Decimal` type given the `step_size` of the
/// `PriceFilter`
pub fn f64_to_decimal(price: f64, step_size: Decimal) -> Decimal {
    let decimal_places = get_num_digits(step_size);
    if decimal_places == 0 {
        return Decimal::from(price as i64);
    }
    let scaling = 10_f64.powi(decimal_places);
    let scaled_f64 = price * scaling;

    if decimal_places > 0 {
        Decimal::from(scaled_f64 as i64) / Decimal::from(10_i32.pow(decimal_places as u32))
    } else {
        Decimal::from(scaled_f64 as i64)
    }
}

/// Sum an iterator of `Decimal` values.
pub(crate) fn decimal_sum<I>(vals: I) -> Decimal
where
    I: IntoIterator<Item = Decimal>,
{
    let mut out = Dec!(0);
    for v in vals.into_iter() {
        out += v;
    }
    out
}

/// Apply a power to the `Decimal` value
pub(crate) fn decimal_pow(val: Decimal, pow: u32) -> Decimal {
    if pow == 0 {
        return Decimal::ONE;
    }
    let mut out = val;
    for _ in 1..pow {
        out *= val;
    }
    out
}

/// Take the square root of a `Decimal` value.
pub(crate) fn decimal_sqrt(val: Decimal) -> Decimal {
    f64_to_decimal(decimal_to_f64(val).sqrt(), Dec!(0.0000001))
}

/// Compute the variance of `Decimal` values
pub(crate) fn variance(vals: &[Decimal]) -> Decimal {
    let n: Decimal = (vals.len() as u64).into();
    let avg: Decimal = decimal_sum(vals.iter().cloned()) / n;
    decimal_sum(vals.iter().map(|v| (v - avg) * (v - avg))) / n
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
    fn test_get_num_digits() {
        assert_eq!(get_num_digits(Dec!(999.0)), -2);
        assert_eq!(get_num_digits(Dec!(100.0)), -2);
        assert_eq!(get_num_digits(Dec!(99.0)), -1);
        assert_eq!(get_num_digits(Dec!(10.0)), -1);
        assert_eq!(get_num_digits(Dec!(9.0)), 0);
        assert_eq!(get_num_digits(Dec!(1.0)), 0);
        assert_eq!(get_num_digits(Dec!(0.0)), 0);
        assert_eq!(get_num_digits(Dec!(0.1)), 1);
        assert_eq!(get_num_digits(Dec!(0.9)), 1);
        assert_eq!(get_num_digits(Dec!(0.01)), 2);
        assert_eq!(get_num_digits(Dec!(0.09)), 2);
        assert_eq!(get_num_digits(Dec!(0.001)), 3);
        assert_eq!(get_num_digits(Dec!(0.009)), 3);
        assert_eq!(get_num_digits(Dec!(0.0001)), 4);
        assert_eq!(get_num_digits(Dec!(0.0009)), 4);
    }

    #[test]
    fn test_decimal_to_f64() {
        let _ = pretty_env_logger::try_init();

        assert_eq!(decimal_to_f64(Decimal::ZERO), 0.0);
        assert_eq!(decimal_to_f64(Decimal::ONE), 1.0);
        assert_eq!(decimal_to_f64(Decimal::TWO), 2.0);

        let mut rng = thread_rng();
        const ROUNDING: i32 = 8;
        for _i in 0..1_000 {
            let val: f64 = rng.gen();
            assert_eq!(
                round(decimal_to_f64(Decimal::try_from(val).unwrap()), ROUNDING),
                round(val, ROUNDING)
            );
            assert_eq!(
                round(
                    decimal_to_f64(Decimal::try_from(val * 10.0).unwrap()),
                    ROUNDING
                ),
                round(val * 10.0, ROUNDING)
            );
            assert_eq!(
                round(
                    decimal_to_f64(Decimal::try_from(val * 100.0).unwrap()),
                    ROUNDING
                ),
                round(val * 100.0, ROUNDING)
            );
        }
    }

    #[test]
    fn test_convert_f64_to_decimal() {
        assert_eq!(f64_to_decimal(0.0, Dec!(100)), Dec!(0));
        assert_eq!(f64_to_decimal(0.0, Dec!(10)), Dec!(0));
        assert_eq!(f64_to_decimal(0.0, Dec!(1)), Dec!(0));
        assert_eq!(f64_to_decimal(0.0, Dec!(0.1)), Dec!(0));
        assert_eq!(f64_to_decimal(0.0, Dec!(0.01)), Dec!(0));

        assert_eq!(f64_to_decimal(5.0, Dec!(100)), Dec!(0));
        assert_eq!(f64_to_decimal(5.0, Dec!(10)), Dec!(0));
        assert_eq!(f64_to_decimal(5.0, Dec!(1)), Dec!(5));
        assert_eq!(f64_to_decimal(5.0, Dec!(0.1)), Dec!(5));
        assert_eq!(f64_to_decimal(5.0, Dec!(0.01)), Dec!(5));

        assert_eq!(f64_to_decimal(0.5, Dec!(100)), Dec!(0));
        assert_eq!(f64_to_decimal(0.5, Dec!(10)), Dec!(0));
        assert_eq!(f64_to_decimal(0.5, Dec!(1)), Dec!(0));
        assert_eq!(f64_to_decimal(0.5, Dec!(0.1)), Dec!(0.5));
        assert_eq!(f64_to_decimal(0.5, Dec!(0.01)), Dec!(0.5));
    }

    #[test]
    fn test_decimal_sum() {
        let vals: Vec<Decimal> = (0..100).map(|v| v.into()).collect();
        assert_eq!(decimal_sum(vals), Dec!(4950));
    }

    #[test]
    fn test_decimal_pow() {
        assert_eq!(decimal_pow(Dec!(2), 0), Dec!(1));
        assert_eq!(decimal_pow(Dec!(2), 1), Dec!(2));
        assert_eq!(decimal_pow(Dec!(2), 2), Dec!(4));
        assert_eq!(decimal_pow(Dec!(2), 3), Dec!(8));

        assert_eq!(decimal_pow(Dec!(0.5), 0), Dec!(1));
        assert_eq!(decimal_pow(Dec!(0.5), 1), Dec!(0.5));
        assert_eq!(decimal_pow(Dec!(0.5), 2), Dec!(0.25));
        assert_eq!(decimal_pow(Dec!(0.5), 3), Dec!(0.125));
    }

    #[test]
    fn test_variance() {
        let vals = &[Dec!(0.5), Dec!(-0.5), Dec!(0.5), Dec!(-0.5)];
        assert_eq!(variance(vals), Dec!(0.25));
    }
}
