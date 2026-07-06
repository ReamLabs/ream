use crate::id::{NUMBER_OF_COLUMNS, column_indices};

/// Which of a block's columns this node holds, against the set it is responsible
/// for.
///
/// Both fields are 128-bit presence bitmaps — bit `i` set ⇔ column index `i`:
/// - `held`: columns actually stored here.
/// - `expected`: columns this node is responsible for (its custody set). For the full-custody MVP
///   the store stamps every value with `ALL_COLUMNS_MASK`; once custody groups land it stamps the
///   node's actual custody set instead, and the query methods below keep working unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaAvailability {
    held: u128,
    expected: u128,
}

impl DaAvailability {
    pub fn new(held: u128, expected: u128) -> Self {
        Self { held, expected }
    }

    /// Whether every column this node is responsible for is held.
    pub fn is_complete(&self) -> bool {
        self.held & self.expected == self.expected
    }

    /// Number of columns physically held, regardless of custody.
    pub fn held_count(&self) -> u64 {
        u64::from(self.held.count_ones())
    }

    /// Whether column `index` is physically held, regardless of custody.
    ///
    /// A pure bitmap probe — this is the cheap presence check for callers that
    /// would otherwise fetch a whole column just to see if it exists. An
    /// out-of-range index is never held.
    pub fn holds(&self, index: u64) -> bool {
        index < NUMBER_OF_COLUMNS && self.held & (1u128 << index) != 0
    }

    /// Column indices this node is responsible for but does not yet hold, in
    /// ascending order.
    ///
    /// This is the list a fetcher turns into a request for the missing columns
    pub fn missing_indices(&self) -> Vec<u64> {
        column_indices(self.expected & !self.held)
    }

    /// Column indices physically held, ascending — the list a serving node walks
    /// to return every column it has for a block. Includes any held outside the
    /// custody set.
    pub fn held_indices(&self) -> Vec<u64> {
        column_indices(self.held)
    }
}

#[cfg(test)]
mod tests {
    use super::DaAvailability;

    /// A small custody set — columns {0, 1, 2, 3} — keeps the expectations
    /// readable while still exercising partial/complete logic.
    const EXPECTED_FOUR: u128 = 0b1111;

    #[test]
    fn holds_probes_single_columns() {
        // Held {0, 2}: bit probes answer per column, and an out-of-range index
        // (>= NUMBER_OF_COLUMNS) is never held.
        let availability = DaAvailability::new(0b0101, EXPECTED_FOUR);
        assert!(availability.holds(0));
        assert!(!availability.holds(1));
        assert!(availability.holds(2));
        assert!(!availability.holds(127));
        assert!(!availability.holds(128));
    }

    #[test]
    fn complete_when_every_expected_column_is_held() {
        let availability = DaAvailability::new(0b1111, EXPECTED_FOUR);
        assert!(availability.is_complete());
        assert_eq!(availability.held_count(), 4);
        assert!(availability.missing_indices().is_empty());
    }

    #[test]
    fn empty_holds_nothing() {
        let availability = DaAvailability::new(0, EXPECTED_FOUR);
        assert!(!availability.is_complete());
        assert_eq!(availability.held_count(), 0);
        assert_eq!(availability.missing_indices(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn partial_reports_only_the_gaps() {
        // Holds columns 0 and 2 of the four expected.
        let availability = DaAvailability::new(0b0101, EXPECTED_FOUR);
        assert!(!availability.is_complete());
        assert_eq!(availability.held_count(), 2);
        assert_eq!(availability.missing_indices(), vec![1, 3]);
    }

    #[test]
    fn extra_columns_beyond_custody_still_complete() {
        // Holds column 4 on top of the expected four: a superset, still complete,
        // and column 4 is never reported as missing.
        let availability = DaAvailability::new(0b11111, EXPECTED_FOUR);
        assert!(availability.is_complete());
        assert_eq!(availability.held_count(), 5);
        assert!(availability.missing_indices().is_empty());
    }

    #[test]
    fn sparse_custody_follows_the_bits_not_the_count() {
        // Custody indices {5, 70, 99}, plus a held column (9) that lies
        // OUTSIDE custody.
        let expected = (1u128 << 5) | (1u128 << 70) | (1u128 << 99);
        let held = (1u128 << 5) | (1u128 << 9);
        let availability = DaAvailability::new(held, expected);

        assert!(!availability.is_complete());
        assert_eq!(availability.held_count(), 2);
        assert_eq!(availability.missing_indices(), vec![70, 99]);
    }

    #[test]
    fn held_indices_lists_every_stored_column_in_order() {
        // Held {0, 2} within custody plus {9} outside it — all count as held.
        let availability = DaAvailability::new((1 << 0) | (1 << 2) | (1 << 9), EXPECTED_FOUR);
        assert_eq!(availability.held_indices(), vec![0, 2, 9]);
        // Nothing held -> empty list.
        assert!(
            DaAvailability::new(0, EXPECTED_FOUR)
                .held_indices()
                .is_empty()
        );
    }
}
