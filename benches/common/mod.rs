//! Shared configuration for all benchmarks.
//!
//! Criterion's defaults (3s warm up, 5s measurement, 100 samples) cost about
//! 9s per benchmark id. These are microbenchmarks that only need a rough
//! estimate, so a 1s warm up and a 2s measurement keep the signal while
//! making the whole suite 3-4x faster.

#![allow(dead_code, reason = "not every benchmark uses every helper")]

use std::{
    num::{
        NonZeroU16,
        NonZeroU32,
    },
    time::Duration,
};

use const_decimal::Decimal;
use criterion::{
    BenchmarkGroup,
    Criterion,
    measurement::WallTime,
};
use lfest::{
    EXPECT_CONFIG,
    EXPECT_CONTRACT_SPEC,
    EXPECT_DECIMAL,
    EXPECT_QUANTITY_FILTER,
    prelude::*,
    test_fee_maker,
    test_fee_taker,
};

/// Samples per benchmark.
///
/// Criterion requires at least 10; 20 is enough to estimate the mean of a
/// microbenchmark without spending seconds on statistics.
pub(crate) const SAMPLES: usize = 20;

/// Warm up time per benchmark.
pub(crate) const WARM_UP: Duration = Duration::from_secs(1);

/// Measurement time per benchmark.
pub(crate) const MEASUREMENT: Duration = Duration::from_secs(2);

/// Create a benchmark group with the shared criterion settings.
pub(crate) fn group<'a>(c: &'a mut Criterion, name: &str) -> BenchmarkGroup<'a, WallTime> {
    let mut group = c.benchmark_group(name);
    group
        .sample_size(SAMPLES)
        .warm_up_time(WARM_UP)
        .measurement_time(MEASUREMENT);
    group
}

/// Log-spaced values from 1 to `max` (inclusive), with roughly 4x steps.
///
/// A linear sweep (`1..100`) spends most of its benchmark ids on steps that
/// are smaller than the timer noise; four to six points are enough to see
/// whether a routine is linear, logarithmic or constant.
#[must_use]
pub(crate) fn sweep(max: usize) -> Vec<usize> {
    assert!(max >= 1, "sweep needs at least one point: max={max}");
    let points = Vec::from_iter(
        std::iter::successors(Some(1_usize), |n| n.checked_mul(4))
            .take_while(|n| *n < max)
            .chain([max]),
    );
    assert_eq!(points[0], 1, "sweep always starts at 1");
    assert_eq!(
        *points.last().expect("sweep is never empty"),
        max,
        "sweep always ends at the maximum"
    );
    assert!(
        points.windows(2).all(|pair| pair[0] < pair[1]),
        "sweep must be strictly increasing: {points:?}"
    );
    points
}

/// Decimal precision used by the order path benchmarks.
pub(crate) const DECIMALS: u8 = 5;

/// A linear futures exchange for the order path benchmarks.
///
/// Size is denoted in `BaseCurrency` and margin in `QuoteCurrency`, like
/// `lfest::mock_exchange_linear`, but with a balance and an order capacity
/// large enough to hold the sweeps (100+ resting orders).
pub(crate) fn linear_exchange(
    max_active_orders: NonZeroU16,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    assert!(max_active_orders.get() > 0);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(5, 1),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).expect(EXPECT_DECIMAL),
        )
        .expect("is a valid price filter"),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).expect(EXPECT_QUANTITY_FILTER),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect(EXPECT_CONTRACT_SPEC);
    let config = Config::new(
        QuoteCurrency::new(100_000, 0),
        max_active_orders,
        contract_spec,
        OrderRateLimits::new(NonZeroU32::MAX),
    )
    .expect(EXPECT_CONFIG);
    Exchange::new(config)
}
