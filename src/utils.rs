use const_decimal::Decimal;

use crate::prelude::*;

/// When no user specified order id is required.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoUserOrderId;

impl std::fmt::Display for NoUserOrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoUserId")
    }
}

/// Return the minimum of two values
#[inline(always)]
pub(crate) fn min<T>(v0: T, v1: T) -> T
where
    T: PartialOrd,
{
    if v0 < v1 { v0 } else { v1 }
}

/// Return the maximum of two values
#[inline(always)]
pub(crate) fn max<T>(v0: T, v1: T) -> T
where
    T: PartialOrd,
{
    if v0 > v1 { v0 } else { v1 }
}

/// Create a `Decimal` from an `f64` value.
// TODO: maybe upstream this impl to `const_decimal`
#[inline(always)]
pub fn decimal_from_f64<I: Mon<D>, const D: u8>(val: f64) -> Option<Decimal<I, D>> {
    let scaling_factor = 10_f64.powi(D as i32);
    let scaled: f64 = (val * scaling_factor).round();
    Decimal::try_from_scaled(I::from(scaled as i64)?, D)
}

/// Scales the value from one range into another
///
/// # Arguments:
/// `from_min`: The minimum value of the origin range
/// `from_max`: The maximum value of the origin range
/// `to_min`: The minimum value of the target range
/// `to_max`: The maximum value of the target range
/// `value`: The value to translate from one range into the other
///
/// # Returns:
/// The scaled value
///
/// Used in benchmarks
#[inline(always)]
pub fn scale<F: num::Float>(from_min: F, from_max: F, to_min: F, to_max: F, value: F) -> F {
    assert2::debug_assert!(from_min <= from_max);
    assert2::debug_assert!(to_min <= to_max);
    to_min + ((value - from_min) * (to_max - to_min)) / (from_max - from_min)
}

/// The margin requirement for all the tracked orders.
#[inline]
pub(crate) fn order_margin<I, const D: u8, BaseOrQuote>(
    bids_notional: BaseOrQuote::PairedCurrency,
    asks_notional: BaseOrQuote::PairedCurrency,
    init_margin_req: Decimal<I, D>,
    position: &Position<I, D, BaseOrQuote>,
) -> BaseOrQuote::PairedCurrency
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    assert2::debug_assert!(init_margin_req > Decimal::zero());
    assert2::debug_assert!(init_margin_req <= Decimal::one());

    use Position::*;
    match position {
        Neutral => max(bids_notional, asks_notional) * init_margin_req,
        Long(inner) => max(bids_notional, asks_notional - inner.notional()) * init_margin_req,
        Short(inner) => max(bids_notional - inner.notional(), asks_notional) * init_margin_req,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use const_decimal::Decimal;
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn scale_proptest(a in 0..100) {
            assert_eq!(scale(0.0, 1.0, 0.0, 100.0, a as f64), a as f64 * 100.0);
            assert_eq!(scale(0.0, 100.0, 0.0, 1.0, a as f64), a as f64 / 100.0);
        }

    }

    #[test]
    fn scale_test() {
        assert_eq!(scale(-1.0, 1.0, 0.0, 1.0, 0.5), 0.75);
        assert_eq!(scale(-1.0, 1.0, 0.0, 1.0, -0.5), 0.25);
        assert_eq!(scale(-1.0, 1.0, -1.0, 1.0, 0.5), 0.5);
    }

    #[test]
    fn test_convert_decimals() {
        assert_eq!(
            Decimal::<i32, 0>::try_from_scaled(10, 1)
                .expect("can convert")
                .0,
            1
        );
    }

    #[test]
    fn test_decimal_from_f64() {
        assert_eq!(
            decimal_from_f64(3.0).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(3, 0).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.1).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(31, 1).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.14).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(314, 2).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.141).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(3141, 3).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.1415).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(31415, 4).unwrap()
        );
        assert_eq!(
            decimal_from_f64(3.14159).unwrap(),
            Decimal::<i64, 5>::try_from_scaled(314159, 5).unwrap()
        );
    }

    #[test]
    fn no_user_order_id_display() {
        assert_eq!(&NoUserOrderId.to_string(), "NoUserId");
    }
}
