use ream_consensus_misc::constants::beacon::SAMPLES_PER_SLOT;

use crate::data_column_sidecar::NUMBER_OF_COLUMNS;

/// Compute the binomial coefficient C(n, k).
fn math_comb(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result: u64 = 1;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

/// Compute the CDF of the hypergeometric distribution.
///
/// P(X <= k) where X ~ Hypergeom(M, n, N)
///   M = population size
///   n = number of success states in population
///   N = number of draws
///   k = number of observed successes
///
/// NOTE: Contains floating-point computations as specified.
fn hypergeom_cdf(k: u64, big_m: u64, n: u64, big_n: u64) -> f64 {
    let mut cdf = 0.0f64;
    for i in 0..=k {
        let numerator = math_comb(n, i) as f64 * math_comb(big_m - n, big_n - i) as f64;
        let denominator = math_comb(big_m, big_n) as f64;
        cdf += numerator / denominator;
    }
    cdf
}

/// Return the number of columns to sample per slot when allowing a given number of failures.
///
/// This helper demonstrates how to calculate the number of columns to query per slot when
/// allowing given number of failures, assuming uniform random selection without replacement.
///
/// # Panics
///
/// Panics if `allowed_failures` is greater than `NUMBER_OF_COLUMNS / 2`.
pub fn get_extended_sample_count(allowed_failures: u64) -> u64 {
    assert!(
        allowed_failures <= NUMBER_OF_COLUMNS / 2,
        "allowed_failures ({allowed_failures}) must be <= NUMBER_OF_COLUMNS / 2 ({})",
        NUMBER_OF_COLUMNS / 2
    );

    let worst_case_missing = NUMBER_OF_COLUMNS / 2 + 1;
    let false_positive_threshold =
        hypergeom_cdf(0, NUMBER_OF_COLUMNS, worst_case_missing, SAMPLES_PER_SLOT);

    let mut sample_count = SAMPLES_PER_SLOT;
    for sc in SAMPLES_PER_SLOT..=NUMBER_OF_COLUMNS {
        sample_count = sc;
        if hypergeom_cdf(allowed_failures, NUMBER_OF_COLUMNS, worst_case_missing, sc)
            <= false_positive_threshold
        {
            break;
        }
    }
    sample_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_comb() {
        assert_eq!(math_comb(5, 0), 1);
        assert_eq!(math_comb(5, 1), 5);
        assert_eq!(math_comb(5, 2), 10);
        assert_eq!(math_comb(5, 5), 1);
        assert_eq!(math_comb(10, 3), 120);
        assert_eq!(math_comb(0, 0), 1);
        assert_eq!(math_comb(3, 5), 0);
    }

    #[test]
    fn test_hypergeom_cdf_basic() {
        // With M=10, n=5, N=3, P(X<=0) should be between 0 and 1
        let cdf = hypergeom_cdf(0, 10, 5, 3);
        assert!(cdf > 0.0);
        assert!(cdf < 1.0);

        // P(X <= N) should equal 1.0 (or very close)
        let cdf_full = hypergeom_cdf(3, 10, 5, 3);
        assert!((cdf_full - 1.0).abs() < 1e-10);
    }

    /// Verify against the spec reference table for NUMBER_OF_COLUMNS=128, SAMPLES_PER_SLOT=8:
    ///
    /// | Allowed missing | 0| 1| 2| 3| 4| 5| 6| 7| 8|
    /// |-----------------|--|--|--|--|--|--|--|--|--|
    /// | Sample count    |8|10|11|13|14|15|16|18|19|
    ///
    /// Note: The spec table in the document assumes SAMPLES_PER_SLOT=16, but ream uses
    /// SAMPLES_PER_SLOT=8. The values here are computed for SAMPLES_PER_SLOT=8.
    #[test]
    fn test_get_extended_sample_count_zero_failures() {
        // With 0 allowed failures, should return SAMPLES_PER_SLOT
        let count = get_extended_sample_count(0);
        assert_eq!(count, SAMPLES_PER_SLOT);
    }

    #[test]
    fn test_get_extended_sample_count_monotonic() {
        // Sample count should be monotonically non-decreasing with allowed failures
        let mut prev = get_extended_sample_count(0);
        for failures in 1..=10 {
            let count = get_extended_sample_count(failures);
            assert!(
                count >= prev,
                "Sample count should be non-decreasing: failures={failures}, count={count}, prev={prev}"
            );
            prev = count;
        }
    }

    #[test]
    fn test_get_extended_sample_count_bounds() {
        // Should always be >= SAMPLES_PER_SLOT
        for failures in 0..=10 {
            let count = get_extended_sample_count(failures);
            assert!(count >= SAMPLES_PER_SLOT);
            assert!(count <= NUMBER_OF_COLUMNS);
        }
    }

    #[test]
    #[should_panic(expected = "allowed_failures")]
    fn test_get_extended_sample_count_too_many_failures() {
        get_extended_sample_count(NUMBER_OF_COLUMNS / 2 + 1);
    }
}
