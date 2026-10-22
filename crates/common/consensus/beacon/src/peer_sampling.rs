use ream_consensus_misc::constants::beacon::SAMPLES_PER_SLOT;

use crate::data_column_sidecar::NUMBER_OF_COLUMNS;

/// Computes combinations (binomial coefficient) `total_items choose chosen_items` as `f64`.
/// Returns `0.0` if `chosen_items > total_items`.
pub fn combinations(total_items: u64, chosen_items: u64) -> f64 {
    if chosen_items > total_items {
        return 0.0;
    }
    let chosen_items = chosen_items.min(total_items - chosen_items);
    let mut result = 1.0;
    for index in 0..chosen_items {
        result = result * (total_items - index) as f64 / (index + 1) as f64;
    }
    result
}

/// Cumulative distribution function for the hypergeometric distribution.
///
/// Python reference: `sum([math_comb(n, i) * math_comb(M - n, N - i) / math_comb(M, N) for i in
/// range(k + 1)])`
pub fn hypergeometric_cdf(
    allowed_successes: u64,
    success_states: u64,
    population_size: u64,
    sample_size: u64,
) -> f64 {
    let denominator = combinations(population_size, sample_size);
    if denominator == 0.0 {
        return 0.0;
    }

    let mut cumulative_probability = 0.0;
    for index in 0..=allowed_successes.min(success_states) {
        if sample_size >= index && (sample_size - index) <= (population_size - success_states) {
            let numerator = combinations(success_states, index)
                * combinations(population_size - success_states, sample_size - index);
            cumulative_probability += numerator / denominator;
        }
    }
    cumulative_probability
}

/// Calculates the number of columns to query per slot when allowing a given number of failures.
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
        hypergeometric_cdf(0, worst_case_missing, NUMBER_OF_COLUMNS, SAMPLES_PER_SLOT);

    for sample_count in SAMPLES_PER_SLOT..=NUMBER_OF_COLUMNS {
        if hypergeometric_cdf(
            allowed_failures,
            worst_case_missing,
            NUMBER_OF_COLUMNS,
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
    fn test_combinations() {
        assert_eq!(combinations(5, 0), 1.0);
        assert_eq!(combinations(5, 1), 5.0);
        assert_eq!(combinations(5, 2), 10.0);
        assert_eq!(combinations(5, 3), 10.0);
        assert_eq!(combinations(5, 4), 5.0);
        assert_eq!(combinations(5, 5), 1.0);
        assert_eq!(combinations(5, 6), 0.0);
        assert_eq!(combinations(0, 0), 1.0);
        assert_eq!(combinations(128, 0), 1.0);
        assert_eq!(combinations(128, 1), 128.0);
        let computed_combinations = combinations(128, 64);
        assert!(
            (computed_combinations - 2.3951146041928083e37).abs() / computed_combinations < 1e-12
        );
    }

    #[test]
    fn test_hypergeometric_cdf_edge_cases() {
        let computed_cdf = hypergeometric_cdf(0, 65, 128, 0);
        assert!((computed_cdf - 1.0).abs() < 1e-9);

        let threshold = hypergeometric_cdf(0, 65, 128, 8);
        assert!((threshold - 0.0027088812421930428).abs() < 1e-12);
    }

    #[test]
    fn test_get_extended_sample_count_zero_failures() {
        assert_eq!(get_extended_sample_count(0), 8);
    }

    #[test]
    fn test_get_extended_sample_count_spec_values() {
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
