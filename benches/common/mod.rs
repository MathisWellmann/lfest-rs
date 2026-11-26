//! Shared configuration for all benchmarks.
//!
//! Criterion's defaults (3s warm up, 5s measurement, 100 samples) cost about
//! 9s per benchmark id. These are microbenchmarks that only need a rough
//! estimate, so a 1s warm up and a 2s measurement keep the signal while
//! making the whole suite 3-4x faster.

#![allow(dead_code, reason = "not every benchmark uses every helper")]

use std::time::Duration;

use criterion::{
    BenchmarkGroup,
    Criterion,
    measurement::WallTime,
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
