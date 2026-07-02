use std::collections::{HashMap, HashSet};

use alloy_primitives::B256;
use ream_consensus_beacon::{
    data_column_sidecar::NUMBER_OF_COLUMNS,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
};

use crate::{PendingAvailability, PendingBlock};

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

    pub fn supernode() -> Self {
        Self::new((0..NUMBER_OF_COLUMNS).collect())
    }

    pub fn insert_pending(
        &mut self,
        block_root: B256,
        signed_block: SignedBeaconBlock,
        post_state: State,
    ) -> Option<PendingBlock<State>> {
        self.entries.entry(block_root).or_default().pending_block = Some(PendingBlock {
            signed_block,
            post_state,
        });
        self.take_if_complete(block_root)
    }

    pub fn add_column(
        &mut self,
        block_root: B256,
        column_index: u64,
    ) -> Option<PendingBlock<State>> {
        if !self.required_columns.contains(&column_index) {
            return None;
        }

        self.entries
            .entry(block_root)
            .or_default()
            .received_columns
            .insert(column_index);
        self.take_if_complete(block_root)
    }

    pub fn remove(&mut self, block_root: &B256) -> Option<PendingAvailability<State>> {
        self.entries.remove(block_root)
    }

    pub fn contains(&self, block_root: &B256) -> bool {
        self.entries.contains_key(block_root)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn take_if_complete(&mut self, block_root: B256) -> Option<PendingBlock<State>> {
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

        let available = checker.insert_pending(root, block_with_blobs(0), ());

        assert!(available.is_some());
        assert!(checker.is_empty());
    }

    #[test]
    fn block_waits_for_all_required_columns() {
        let mut checker = checker(&[0, 1, 2]);
        let root = B256::repeat_byte(2);

        assert!(
            checker
                .insert_pending(root, block_with_blobs(1), ())
                .is_none()
        );
        assert!(checker.add_column(root, 0).is_none());
        assert!(checker.add_column(root, 1).is_none());

        let available = checker.add_column(root, 2);
        assert!(available.is_some());
        assert!(!checker.contains(&root));
    }

    #[test]
    fn columns_arriving_before_block_complete_it_on_insert() {
        let mut checker = checker(&[0, 1]);
        let root = B256::repeat_byte(3);

        assert!(checker.add_column(root, 0).is_none());
        assert!(checker.add_column(root, 1).is_none());

        let available = checker.insert_pending(root, block_with_blobs(1), ());
        assert!(available.is_some());
        assert!(checker.is_empty());
    }

    #[test]
    fn duplicate_columns_do_not_count_twice() {
        let mut checker = checker(&[0, 1]);
        let root = B256::repeat_byte(4);

        checker.insert_pending(root, block_with_blobs(1), ());
        assert!(checker.add_column(root, 0).is_none());
        assert!(checker.add_column(root, 0).is_none());
        assert!(checker.add_column(root, 1).is_some());
    }

    #[test]
    fn columns_without_a_block_stay_pending() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(5);

        assert!(checker.add_column(root, 0).is_none());
        assert!(checker.contains(&root));
    }

    #[test]
    fn columns_outside_the_required_set_do_not_complete() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(6);

        checker.insert_pending(root, block_with_blobs(1), ());
        assert!(checker.add_column(root, 5).is_none());
        assert!(checker.add_column(root, 0).is_some());
    }

    #[test]
    fn columns_outside_the_required_set_do_not_create_entries() {
        let mut checker = checker(&[0]);
        let root = B256::repeat_byte(7);

        assert!(checker.add_column(root, 5).is_none());
        assert!(checker.is_empty());
    }

    #[test]
    fn supernode_requires_all_128_columns() {
        let mut checker: DataAvailabilityChecker<()> = DataAvailabilityChecker::supernode();
        let root = B256::repeat_byte(8);

        checker.insert_pending(root, block_with_blobs(1), ());
        for index in 0..NUMBER_OF_COLUMNS - 1 {
            assert!(checker.add_column(root, index).is_none());
        }
        assert!(checker.add_column(root, NUMBER_OF_COLUMNS - 1).is_some());
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
