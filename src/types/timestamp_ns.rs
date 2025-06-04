use std::fmt::Display;

use derive_more::{Add, AddAssign, Div, Mul, Sub};

pub(crate) const NANOS_PER_SECOND: i64 = 1_000_000_000;

/// The type of a timestamp that is measured in nanoseconds.
#[derive(
    Default, Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Add, Sub, Div, AddAssign, Mul,
)]
#[div(forward)]
#[mul(forward)]
#[repr(transparent)]
pub struct TimestampNs(i64);

impl TimestampNs {
    /// Floor to the nearest second.
    #[inline(always)]
    pub fn floor_to_nearest_second(self) -> Self {
        (self.0 - self.0 % NANOS_PER_SECOND).into()
    }
}

impl From<i64> for TimestampNs {
    #[inline(always)]
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<TimestampNs> for i64 {
    #[inline(always)]
    fn from(val: TimestampNs) -> Self {
        val.0
    }
}

impl Display for TimestampNs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<i64> for TimestampNs {
    #[inline(always)]
    fn as_ref(&self) -> &i64 {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn timestamp_ns_floor_to_nearest_second() {
        // let now = std::time::SystemTime::now();
        // let since_the_epoch = now.duration_since(std::time::UNIX_EPOCH).expect("Time went backwards");

        // println!("{}", since_the_epoch.as_nanos());

        let ts = TimestampNs::from(1742475657135330098);
        assert_eq!(ts.floor_to_nearest_second(), 1742475657000000000.into());
        assert_eq!(i64::from(ts), 1742475657135330098)
    }
}
