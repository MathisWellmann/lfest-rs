use std::num::NonZeroU32;

use crate::types::TimestampNs;

#[derive(Debug, Clone, thiserror::Error, derive_more::Display, Eq, PartialEq)]
pub struct RateLimitReached;

/// Limits the rate at which limit orders can be submitted.
/// Operates on buckets measured in seconds.
#[derive(Clone, Debug)]
pub(crate) struct OrderRateLimiter {
    /// The start of the rate limiting bucket in seconds.
    bucket_start_ns: TimestampNs,
    /// The maximum number of order actions per second.
    orders_per_second: u32,
    /// The number of remaining order actions that can be submitted during the period.
    remaining: u32,
}

impl OrderRateLimiter {
    pub(crate) fn new(orders_per_second: NonZeroU32) -> Self {
        let orders_per_second = orders_per_second.get();
        Self {
            bucket_start_ns: 0.into(),
            orders_per_second,
            remaining: orders_per_second,
        }
    }

    /// If `true`, the `current_ts_ns` falls within the current bucket.
    #[inline(always)]
    fn is_in_bucket(&self, current_ts_ns: TimestampNs) -> bool {
        debug_assert!(
            current_ts_ns >= self.bucket_start_ns,
            "Timestamps are assumed to always increment. Here we don't additionally check for the lower bound of the bucket."
        );
        let bucket_end_ts_ns = self.bucket_start_ns + crate::types::NANOS_PER_SECOND.into();
        current_ts_ns < bucket_end_ts_ns
    }

    /// Set the new bucket start timestamp by rounding to the nearest second.
    #[inline(always)]
    fn new_bucket(&mut self, current_ts_ns: TimestampNs) {
        self.bucket_start_ns = current_ts_ns.floor_to_nearest_second();
        self.remaining = self.orders_per_second;
    }

    /// Acquire a single permit for a new order related action.
    /// returns `true` if the rate limit has been reached.
    #[inline]
    pub(crate) fn aquire(&mut self, current_ts_ns: TimestampNs) -> Result<(), RateLimitReached> {
        if !self.is_in_bucket(current_ts_ns) {
            self.new_bucket(current_ts_ns);
            self.remaining -= 1;
            return Ok(());
        }
        if self.remaining == 0 {
            return Err(RateLimitReached);
        }
        self.remaining -= 1;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EXPECT_NON_ZERO;

    #[test]
    fn order_rate_limiter() {
        let mut limiter = OrderRateLimiter::new(NonZeroU32::new(5).expect(EXPECT_NON_ZERO));
        for _i in 0..5 {
            assert!(limiter.aquire(0.into()).is_ok());
        }
        assert!(limiter.aquire(0.into()).is_err());

        for _i in 0..5 {
            assert!(limiter.aquire(1_000_000_000.into()).is_ok());
        }
        assert!(limiter.aquire(1_000_000_000.into()).is_err());
    }
}
