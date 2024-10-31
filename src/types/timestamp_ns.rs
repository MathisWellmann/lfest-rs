use std::fmt::Display;

use derive_more::{Add, AddAssign, Div, Mul, Sub};

/// The type of a timestamp that is measured in nanoseconds.
#[derive(
    Default, Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Add, Sub, Div, AddAssign, Mul,
)]
#[div(forward)]
#[mul(forward)]
#[repr(transparent)]
pub struct TimestampNs(i64);

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
