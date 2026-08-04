use std::collections::{HashMap, HashSet};

use alloy_primitives::B256;
use ream_consensus_beacon::{
    data_column_sidecar::NUMBER_OF_COLUMNS,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
};

use crate::{PendingAvailability, PendingBlock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityEntryStatus {
    Absent,
    ColumnsOnly,
    PendingBlock,
    Complete,
}

#[derive(Debug)]
pub struct DataAvailabilityChecker<State = BeaconState> {
    entries: HashMap<B256, PendingAvailability<State>>,
    required_columns: HashSet<u64>,
}

impl<State> DataAvailabilityChecker<State> {
    pub fn new(required_columns: HashSet<u64>) -> Self {
        assert!(
            !required_columns.is_empty(),
            "data availability checker must require at least one column"
        );
        assert!(
            required_columns
                .iter()
                .all(|index| *index < NUMBER_OF_COLUMNS),
            "data availability checker column set contains an out-of-range index"
        );

        Self {
            entries: HashMap::new(),
            required_columns,
        }
    }

    // require a node to custody all 128 columns
    pub fn supernode() -> Self {
        Self::new((0..NUMBER_OF_COLUMNS).collect())
    }

    pub fn required_columns(&self) -> &HashSet<u64> {
        &self.required_columns
    }

    pub fn insert_pending(
        &mut self,
        block_root: B256,
        signed_block: SignedBeaconBlock,
        post_state: State,
    ) {
        let entry = self.entries.entry(block_root).or_default();
        entry.slot = signed_block.message.slot;
        entry.pending_block = Some(PendingBlock {
            signed_block,
            post_state,
        });
    }

    pub fn add_column(&mut self, block_root: B256, column_index: u64, slot: u64) {
        if !self.required_columns.contains(&column_index) {
            return;
        }

        let entry = self.entries.entry(block_root).or_default();
        if entry.pending_block.is_none() && entry.received_columns.is_empty() {
            entry.slot = slot;
        }
        entry.received_columns.insert(column_index);
    }

    pub fn prune(&mut self, cutoff_slot: u64) -> usize {
        let original_len = self.entries.len();
        self.entries.retain(|_, entry| entry.slot >= cutoff_slot);
        original_len - self.entries.len()
    }

    pub fn remove(&mut self, block_root: &B256) -> Option<PendingAvailability<State>> {
        self.entries.remove(block_root)
    }

    pub fn status(&self, block_root: &B256) -> AvailabilityEntryStatus {
        match self.entries.get(block_root) {
            None => AvailabilityEntryStatus::Absent,
            Some(entry) if self.is_complete(entry) => AvailabilityEntryStatus::Complete,
            Some(entry) if entry.pending_block.is_some() => AvailabilityEntryStatus::PendingBlock,
            Some(_) => AvailabilityEntryStatus::ColumnsOnly,
        }
    }

    pub fn pending_block(&self, block_root: &B256) -> Option<&PendingBlock<State>> {
        self.entries
            .get(block_root)
            .and_then(|entry| entry.pending_block.as_ref())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn take_if_complete(&mut self, block_root: B256) -> Option<PendingBlock<State>> {
        if !self.is_complete(self.entries.get(&block_root)?) {
            return None;
        }

        self.entries
            .remove(&block_root)
            .and_then(|entry| entry.pending_block)
    }

    fn is_complete(&self, entry: &PendingAvailability<State>) -> bool {
        let Some(pending_block) = &entry.pending_block else {
            return false;
        };

        // This means block has no blobs, so it is complete immediately
        if pending_block
            .signed_block
            .message
            .body
            .blob_kzg_commitments
            .is_empty()
        {
            return true;
        }

        self.required_columns.is_subset(&entry.received_columns)
    }
}

#[cfg(test)]
mod tests {
    use ream_consensus_beacon::electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_block_body::BeaconBlockBody,
    };
    use ream_consensus_misc::{
        constants::beacon::BYTES_PER_COMMITMENT,
        polynomial_commitments::kzg_commitment::KZGCommitment,
    };
    use ssz_types::VariableList;

    use super::*;

    fn block_with_blobs(blob_count: usize) -> SignedBeaconBlock {
        let commitments = vec![KZGCommitment([0u8; BYTES_PER_COMMITMENT]); blob_count];
        SignedBeaconBlock {
            message: BeaconBlock {
                body: BeaconBlockBody {
                    blob_kzg_commitments: VariableList::new(commitments).unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
            signature: Default::default(),
        }
    }

    fn checker(required_columns: &[u64]) -> DataAvailabilityChecker<()> {
        DataAvailabilityChecker::new(required_columns.iter().copied().collect())
    }

    #[test]
    fn zero_blob_block_is_available_immediately() {
        let mut checker = checker(&[0, 1, 2]);
        let root = B256::repeat_byte(1);

        checker.insert_pending(root, block_with_blobs(0), ());

        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Complete);
        let available = checker.take_if_complete(root);
        assert!(available.is_some());
        assert!(checker.is_empty());
    }

    #[test]
    fn block_waits_for_all_required_columns() {
        let mut checker = checker(&[0, 1, 2]);
        let root = B256::repeat_byte(2);

        checker.insert_pending(root, block_with_blobs(1), ());
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::PendingBlock);
        checker.add_column(root, 0, 10);
        checker.add_column(root, 1, 10);

        checker.add_column(root, 2, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Complete);
        let available = checker.take_if_complete(root);
        assert!(available.is_some());
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Absent);
    }

    #[test]
    fn columns_arriving_before_block_complete_it_on_insert() {
        let mut checker = checker(&[0, 1]);
        let root = B256::repeat_byte(3);

        checker.add_column(root, 0, 10);
        checker.add_column(root, 1, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::ColumnsOnly);

        checker.insert_pending(root, block_with_blobs(1), ());
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Complete);
        let available = checker.take_if_complete(root);
        assert!(available.is_some());
        assert!(checker.is_empty());
    }

    #[test]
    fn duplicate_columns_do_not_count_twice() {
        let mut checker = checker(&[0, 1]);
        let root = B256::repeat_byte(4);

        checker.insert_pending(root, block_with_blobs(1), ());
        checker.add_column(root, 0, 10);
        checker.add_column(root, 0, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::PendingBlock);
        checker.add_column(root, 1, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Complete);
        assert!(checker.take_if_complete(root).is_some());
    }

    #[test]
    fn columns_without_a_block_stay_pending() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(5);

        checker.add_column(root, 0, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::ColumnsOnly);
    }

    #[test]
    fn columns_outside_the_required_set_do_not_complete() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(6);

        checker.insert_pending(root, block_with_blobs(1), ());
        checker.add_column(root, 5, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::PendingBlock);
        checker.add_column(root, 0, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Complete);
        assert!(checker.take_if_complete(root).is_some());
    }

    #[test]
    fn columns_outside_the_required_set_do_not_create_entries() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(7);

        checker.add_column(root, 5, 10);
        assert!(checker.is_empty());
    }

    #[test]
    fn supernode_requires_all_128_columns() {
        let mut checker: DataAvailabilityChecker<()> = DataAvailabilityChecker::supernode();
        let root = B256::repeat_byte(8);

        checker.insert_pending(root, block_with_blobs(1), ());
        for index in 0..NUMBER_OF_COLUMNS - 1 {
            checker.add_column(root, index, 10);
        }
        checker.add_column(root, NUMBER_OF_COLUMNS - 1, 10);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Complete);
        assert!(checker.take_if_complete(root).is_some());
    }

    #[test]
    fn prune_removes_old_block_entries() {
        let mut checker = checker(&[0]);
        let old_root = B256::repeat_byte(9);
        let new_root = B256::repeat_byte(10);
        let mut old_block = block_with_blobs(1);
        old_block.message.slot = 9;
        let mut new_block = block_with_blobs(1);
        new_block.message.slot = 10;

        checker.insert_pending(old_root, old_block, ());
        checker.insert_pending(new_root, new_block, ());

        assert_eq!(checker.prune(10), 1);
        assert_eq!(checker.status(&old_root), AvailabilityEntryStatus::Absent);
        assert_eq!(
            checker.status(&new_root),
            AvailabilityEntryStatus::PendingBlock
        );
    }

    #[test]
    fn prune_removes_old_orphan_column_entries() {
        let mut checker = checker(&[0]);
        let old_root = B256::repeat_byte(11);
        let new_root = B256::repeat_byte(12);

        checker.add_column(old_root, 0, 9);
        checker.add_column(new_root, 0, 10);

        assert_eq!(checker.prune(10), 1);
        assert_eq!(checker.status(&old_root), AvailabilityEntryStatus::Absent);
        assert_eq!(
            checker.status(&new_root),
            AvailabilityEntryStatus::ColumnsOnly
        );
    }

    #[test]
    fn prune_keeps_entries_at_and_after_cutoff() {
        let mut checker = checker(&[0]);
        let cutoff_root = B256::repeat_byte(13);
        let after_root = B256::repeat_byte(14);

        checker.add_column(cutoff_root, 0, 10);
        checker.add_column(after_root, 0, 11);

        assert_eq!(checker.prune(10), 0);
        assert_eq!(
            checker.status(&cutoff_root),
            AvailabilityEntryStatus::ColumnsOnly
        );
        assert_eq!(
            checker.status(&after_root),
            AvailabilityEntryStatus::ColumnsOnly
        );
    }

    #[test]
    fn add_column_after_prune_creates_a_new_entry() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(15);

        checker.add_column(root, 0, 9);
        assert_eq!(checker.prune(10), 1);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::Absent);

        checker.add_column(root, 0, 11);
        assert_eq!(checker.status(&root), AvailabilityEntryStatus::ColumnsOnly);
    }

    #[test]
    #[should_panic(expected = "at least one column")]
    fn checker_rejects_empty_required_column_set() {
        let _checker: DataAvailabilityChecker<()> = DataAvailabilityChecker::new(HashSet::new());
    }

    #[test]
    #[should_panic(expected = "out-of-range")]
    fn checker_rejects_out_of_range_required_column_set() {
        let _checker: DataAvailabilityChecker<()> =
            DataAvailabilityChecker::new([NUMBER_OF_COLUMNS].into_iter().collect());
    }
}
