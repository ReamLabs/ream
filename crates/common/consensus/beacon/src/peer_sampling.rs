use ream_consensus_misc::constants::beacon::SAMPLES_PER_SLOT;

use crate::data_column_sidecar::NUMBER_OF_COLUMNS;

/// Compute combinations `n choose k` (binomial coefficient) as `f64`.
///
/// Returns `0.0` if `k > n`.
/// Matches the behavior of Python's `math.comb(n, k)`.
pub fn math_comb(n: u64, k: u64) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut r = 1.0;
    for i in 0..k {
        r = r * (n - i) as f64 / (i + 1) as f64;
    }
    r
}

/// Cumulative distribution function for the hypergeometric distribution.
///
/// Computes the probability of observing at most `k` successes in `sample_count` (`N`) draws
/// without replacement from a population of size `total_population` (`M`) containing
/// `success_states` (`n`) successes.
///
/// Matches the Python consensus-specs helper:
/// `sum([math_comb(n, i) * math_comb(M - n, N - i) / math_comb(M, N) for i in range(k + 1)])`
pub fn hypergeom_cdf(k: u64, total_population: u64, success_states: u64, sample_count: u64) -> f64 {
    let denom = math_comb(total_population, sample_count);
    if denom == 0.0 {
        return 0.0;
    }

    let mut sum = 0.0;
    for i in 0..=k {
        if i <= success_states
            && sample_count >= i
            && (sample_count - i) <= (total_population - success_states)
        {
            let num = math_comb(success_states, i)
                * math_comb(total_population - success_states, sample_count - i);
            sum += num / denom;
        }
    }
    sum
}

/// Calculate the number of columns to query per slot when allowing a given number of failures,
/// assuming uniform random selection without replacement, according to the Fulu Peer Sampling spec.
///
/// Spec reference:
/// https://github.com/ethereum/consensus-specs/blob/9d377fd53d029536e57cfda1a4d2c700c59f86bf/specs/fulu/peer-sampling.md#get_extended_sample_count
///
/// # Panics
/// Panics if `allowed_failures > NUMBER_OF_COLUMNS / 2`.
pub fn get_extended_sample_count(allowed_failures: u64) -> u64 {
    assert!(
        allowed_failures <= NUMBER_OF_COLUMNS / 2,
        "allowed_failures ({allowed_failures}) must be <= NUMBER_OF_COLUMNS / 2 ({})",
        NUMBER_OF_COLUMNS / 2
    );

    let worst_case_missing = NUMBER_OF_COLUMNS / 2 + 1;
    let false_positive_threshold =
        hypergeom_cdf(0, NUMBER_OF_COLUMNS, worst_case_missing, SAMPLES_PER_SLOT);

    for sample_count in SAMPLES_PER_SLOT..=NUMBER_OF_COLUMNS {
        if hypergeom_cdf(
            allowed_failures,
            NUMBER_OF_COLUMNS,
            worst_case_missing,
            sample_count,
        ) <= false_positive_threshold
        {
            return sample_count;
        }
    }

    NUMBER_OF_COLUMNS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_comb() {
        assert_eq!(math_comb(5, 0), 1.0);
        assert_eq!(math_comb(5, 1), 5.0);
        assert_eq!(math_comb(5, 2), 10.0);
        assert_eq!(math_comb(5, 3), 10.0);
        assert_eq!(math_comb(5, 4), 5.0);
        assert_eq!(math_comb(5, 5), 1.0);
        assert_eq!(math_comb(5, 6), 0.0);
        assert_eq!(math_comb(0, 0), 1.0);
        assert_eq!(math_comb(128, 0), 1.0);
        assert_eq!(math_comb(128, 1), 128.0);
        // Verify comb(128, 64) matches exact value: ~2.395114604192808e+37
        let c = math_comb(128, 64);
        assert!((c - 2.3951146041928083e37).abs() / c < 1e-12);
    }

    #[test]
    fn test_hypergeom_cdf_edge_cases() {
        // At 0 samples, probability of <= 0 successes is 1.0
        let cdf = hypergeom_cdf(0, 128, 65, 0);
        assert!((cdf - 1.0).abs() < 1e-9);

        // Threshold for (0, 128, 65, 8) matches the Python consensus-specs value: ~0.0027088812421930428
        let threshold = hypergeom_cdf(0, 128, 65, 8);
        assert!((threshold - 0.0027088812421930428).abs() < 1e-12);
    }

    #[test]
    fn test_get_extended_sample_count_zero_failures() {
        // 0 allowed failures must equal SAMPLES_PER_SLOT (8)
        assert_eq!(get_extended_sample_count(0), 8);
    }

    #[test]
    fn test_get_extended_sample_count_spec_values() {
        // Test exact expected sample counts computed from official Fulu peer-sampling spec reference
        let expected_samples = [
            (0, 8),
            (1, 12),
            (2, 15),
            (3, 18),
            (4, 20),
            (5, 23),
            (6, 25),
            (7, 28),
            (8, 30),
            (9, 32),
            (10, 35),
            (15, 46),
            (20, 56),
            (30, 76),
            (40, 94),
            (50, 110),
            (60, 124),
            (64, 128),
        ];

        for (failures, expected) in expected_samples {
            assert_eq!(
                get_extended_sample_count(failures),
                expected,
                "Mismatch for allowed_failures = {failures}"
            );
        }
    }

    #[test]
    fn test_get_extended_sample_count_monotonicity() {
        // Sample count must be monotonically non-decreasing with allowed failures
        let mut previous = 0;
        for failures in 0..=(NUMBER_OF_COLUMNS / 2) {
            let count = get_extended_sample_count(failures);
            assert!(
                count >= previous,
                "Sample count decreased from {previous} to {count} at failures {failures}"
            );
            assert!(count >= SAMPLES_PER_SLOT);
            assert!(count <= NUMBER_OF_COLUMNS);
            previous = count;
        }
    }

    #[test]
    #[should_panic(expected = "allowed_failures")]
    fn test_get_extended_sample_count_panics_on_invalid_failures() {
        get_extended_sample_count(NUMBER_OF_COLUMNS / 2 + 1);
    }
}
