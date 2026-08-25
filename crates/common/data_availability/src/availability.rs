use crate::id::{NUMBER_OF_COLUMNS, column_indices};

/// Which of a block's columns this node holds (`held`) against the set it is
/// responsible for (`expected`). Both are 128-bit presence bitmaps: bit `i`
/// set ⇔ column `i`. The full-custody MVP stamps `expected` with
/// `ALL_COLUMNS_MASK`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnAvailability {
    held: u128,
    expected: u128,
}

impl ColumnAvailability {
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

    /// Whether column `index` is held; an out-of-range index is never held.
    pub fn holds(&self, index: u64) -> bool {
        index < NUMBER_OF_COLUMNS && self.held & (1u128 << index) != 0
    }

    /// Column indices expected but not held, ascending.
    pub fn missing_indices(&self) -> Vec<u64> {
        column_indices(self.expected & !self.held)
    }

    /// Column indices held (including any outside custody), ascending.
    pub fn held_indices(&self) -> Vec<u64> {
        column_indices(self.held)
    }
}

#[cfg(test)]
mod tests {
    use super::ColumnAvailability;

    const EXPECTED_FOUR: u128 = 0b1111;

    #[test]
    fn holds_probes_single_columns() {
        let availability = ColumnAvailability::new(0b0101, EXPECTED_FOUR);
        assert!(availability.holds(0));
        assert!(!availability.holds(1));
        assert!(availability.holds(2));
        assert!(!availability.holds(127));
        assert!(!availability.holds(128));
    }

    #[test]
    fn complete_when_every_expected_column_is_held() {
        let availability = ColumnAvailability::new(0b1111, EXPECTED_FOUR);
        assert!(availability.is_complete());
        assert_eq!(availability.held_count(), 4);
        assert!(availability.missing_indices().is_empty());
    }

    #[test]
    fn empty_holds_nothing() {
        let availability = ColumnAvailability::new(0, EXPECTED_FOUR);
        assert!(!availability.is_complete());
        assert_eq!(availability.held_count(), 0);
        assert_eq!(availability.missing_indices(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn partial_reports_only_the_gaps() {
        let availability = ColumnAvailability::new(0b0101, EXPECTED_FOUR);
        assert!(!availability.is_complete());
        assert_eq!(availability.held_count(), 2);
        assert_eq!(availability.missing_indices(), vec![1, 3]);
    }

    #[test]
    fn extra_columns_beyond_custody_still_complete() {
        let availability = ColumnAvailability::new(0b11111, EXPECTED_FOUR);
        assert!(availability.is_complete());
        assert_eq!(availability.held_count(), 5);
        assert!(availability.missing_indices().is_empty());
    }

    #[test]
    fn sparse_custody_follows_the_bits_not_the_count() {
        let expected = (1u128 << 5) | (1u128 << 70) | (1u128 << 99);
        let held = (1u128 << 5) | (1u128 << 9);
        let availability = ColumnAvailability::new(held, expected);

        assert!(!availability.is_complete());
        assert_eq!(availability.held_count(), 2);
        assert_eq!(availability.missing_indices(), vec![70, 99]);
    }

    #[test]
    fn held_indices_lists_every_stored_column_in_order() {
        let availability = ColumnAvailability::new((1 << 0) | (1 << 2) | (1 << 9), EXPECTED_FOUR);
        assert_eq!(availability.held_indices(), vec![0, 2, 9]);
        assert!(
            ColumnAvailability::new(0, EXPECTED_FOUR)
                .held_indices()
                .is_empty()
        );
    }
}
