use std::collections::HashMap;

use alloy_primitives::B256;
use parking_lot::RwLock;
use ream_bls::{BLSSignature, traits::Aggregatable};
use ream_validator_beacon::{
    contribution_and_proof::SyncCommitteeContribution, sync_committee::SyncCommitteeMessage,
};
use ssz_types::{BitVector, typenum::U128};
use tracing::warn;
use tree_hash_derive::TreeHash;

#[derive(Debug, Clone, PartialEq, Eq, Hash, TreeHash)]
pub struct SyncCommitteeDataKey {
    pub slot: u64,
    pub beacon_block_root: B256,
    pub subcommittee_index: u64,
}

// Default maximum number of sync committee contributions to store per key
// This prevents memory issues while allowing proper aggregation
// Mainnet: 128, Testnet: 8
const DEFAULT_MAX_SYNC_CONTRIBUTIONS_PER_KEY: usize = 128;

#[derive(Debug)]
pub struct SyncCommitteePool {
    messages: RwLock<HashMap<SyncCommitteeDataKey, Vec<SyncCommitteeMessage>>>,
    contributions: RwLock<HashMap<SyncCommitteeDataKey, Vec<SyncCommitteeContribution>>>,
    max_contributions_per_key: usize,
}

impl Default for SyncCommitteePool {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SYNC_CONTRIBUTIONS_PER_KEY)
    }
}

impl SyncCommitteePool {
    /// Creates a new SyncCommitteePool with the specified maximum contributions per key.
    ///
    /// # Arguments
    /// * `max_contributions_per_key` - Maximum number of contributions to store per key.
    ///   Recommended: 128 for mainnet, 8 for testnets.
    pub fn new(max_contributions_per_key: usize) -> Self {
        Self {
            messages: RwLock::new(HashMap::new()),
            contributions: RwLock::new(HashMap::new()),
            max_contributions_per_key,
        }
    }
    pub fn insert_sync_committee_message(
        &self,
        message: SyncCommitteeMessage,
        subcommittee_index: u64,
    ) {
        // Store raw messages keyed by (slot, root, subcommittee_index)
        let key = SyncCommitteeDataKey {
            slot: message.slot,
            beacon_block_root: message.beacon_block_root,
            subcommittee_index,
        };

        let mut map = self.messages.write();
        let entry = map.entry(key).or_default();
        if entry.len() < self.max_contributions_per_key {
            entry.push(message);
        } else {
            warn!(
                "Sync committee message pool capacity reached ({}) for slot {} (subcommittee_index: {}, block_root: {:?}), dropping message from validator {}",
                self.max_contributions_per_key,
                message.slot,
                subcommittee_index,
                message.beacon_block_root,
                message.validator_index
            );
        }
    }

    pub fn get_sync_committee_contributions(
        &self,
        slot: u64,
        beacon_block_root: B256,
        subcommittee_index: u64,
    ) -> Vec<SyncCommitteeContribution> {
        let key = SyncCommitteeDataKey {
            slot,
            beacon_block_root,
            subcommittee_index,
        };

        self.contributions
            .read()
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_best_sync_committee_contribution(
        &self,
        slot: u64,
        beacon_block_root: B256,
        subcommittee_index: u64,
    ) -> Option<SyncCommitteeContribution> {
        let contributions =
            self.get_sync_committee_contributions(slot, beacon_block_root, subcommittee_index);

        // Select the contribution with the most aggregation bits set (highest participation)
        contributions
            .into_iter()
            .max_by_key(|c| c.aggregation_bits.iter().filter(|b| *b).count())
    }

    pub fn clean_sync_committee_contributions(&self, current_slot: u64) {
        // Keep contributions for current slot and one slot back
        self.contributions
            .write()
            .retain(|key, _| key.slot >= current_slot.saturating_sub(1));
    }

    pub fn clean_sync_committee_messages(&self, current_slot: u64) {
        // Keep messages for current slot and one slot back
        self.messages
            .write()
            .retain(|key, _| key.slot >= current_slot.saturating_sub(1));
    }

    /// Aggregates sync committee messages into contributions.
    ///
    /// This function takes an iterator of (SyncCommitteeMessage, index_in_subcommittee) pairs
    /// and aggregates them into the pool's contributions. It creates or updates aggregates by
    /// combining signatures and setting the appropriate aggregation bits.
    ///
    /// For each message:
    /// - First attempts to aggregate the signature with existing contributions
    /// - Only if aggregation succeeds, sets the corresponding bit in the aggregation bitfield
    /// - Avoids duplicate aggregation if the bit is already set
    ///
    /// This ensures aggregation bits are only set when signatures are successfully aggregated,
    /// preventing inconsistent state where a bit is set without the corresponding signature.
    pub fn aggregate_messages(
        &self,
        slot: u64,
        beacon_block_root: B256,
        subcommittee_index: u64,
        messages: impl IntoIterator<Item = (SyncCommitteeMessage, usize)>,
    ) {
        let key = SyncCommitteeDataKey {
            slot,
            beacon_block_root,
            subcommittee_index,
        };

        let mut contributions_map = self.contributions.write();
        let contributions = contributions_map.entry(key).or_default();

        // Ensure we have at least one aggregate to work with
        if contributions.is_empty() {
            contributions.push(SyncCommitteeContribution {
                slot,
                beacon_block_root,
                subcommittee_index,
                aggregation_bits: BitVector::<U128>::default(),
                signature: BLSSignature::infinity(),
            });
        }

        for (message, index_in_subcommittee) in messages {
            // Skip if any contribution already has this bit set (duplicate check)
            if contributions.iter().any(|c| {
                c.aggregation_bits
                    .get(index_in_subcommittee)
                    .unwrap_or(false)
            }) {
                continue;
            }

            // Try to add this message to an existing contribution
            let mut added = false;

            for contribution in contributions.iter_mut() {
                // First try to aggregate the signatures. Only set the bit if aggregation succeeds.
                match BLSSignature::aggregate(&[&contribution.signature, &message.signature]) {
                    Ok(aggregated_sig) => {
                        // Now set the bit for this validator's position
                        if contribution
                            .aggregation_bits
                            .set(index_in_subcommittee, true)
                            .is_err()
                        {
                            warn!(
                                "Invalid index_in_subcommittee: {} for validator {} at slot {} (subcommittee_index: {}, block_root: {:?})",
                                index_in_subcommittee,
                                message.validator_index,
                                slot,
                                subcommittee_index,
                                beacon_block_root
                            );
                            continue;
                        }

                        contribution.signature = aggregated_sig;
                        added = true;
                        break;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to aggregate signature for validator {} at slot {} (subcommittee_index: {}, block_root: {:?}): {:?}",
                            message.validator_index, slot, subcommittee_index, beacon_block_root, e
                        );
                        continue;
                    }
                }
            }

            // If we couldn't add to any existing contribution and we're under the limit, attempt to
            // create a new one using the message signature alone. Only set the bit if the signature
            // is valid (i.e., can be aggregated/parsed successfully).
            if !added && contributions.len() < self.max_contributions_per_key {
                match BLSSignature::aggregate(&[&message.signature]) {
                    Ok(valid_sig) => {
                        let mut aggregation_bits = BitVector::<U128>::default();
                        if aggregation_bits.set(index_in_subcommittee, true).is_ok() {
                            contributions.push(SyncCommitteeContribution {
                                slot,
                                beacon_block_root,
                                subcommittee_index,
                                aggregation_bits,
                                signature: valid_sig,
                            });
                        } else {
                            warn!(
                                "Invalid index_in_subcommittee: {} for validator {} at slot {} (subcommittee_index: {}, block_root: {:?})",
                                index_in_subcommittee,
                                message.validator_index,
                                slot,
                                subcommittee_index,
                                beacon_block_root
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Skipping new contribution due to invalid signature for validator {} at slot {} (subcommittee_index: {}, block_root: {:?}): {:?}",
                            message.validator_index, slot, subcommittee_index, beacon_block_root, e
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ssz_types::{FixedVector, typenum::U96};

    use super::*;

    #[test]
    fn test_aggregate_messages() {
        let pool = SyncCommitteePool::default();

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Create sync committee messages with different validator indices
        let messages: Vec<(SyncCommitteeMessage, usize)> = vec![
            (
                SyncCommitteeMessage {
                    slot,
                    beacon_block_root: root,
                    validator_index: 10,
                    signature: BLSSignature::infinity(),
                },
                0, // index_in_subcommittee
            ),
            (
                SyncCommitteeMessage {
                    slot,
                    beacon_block_root: root,
                    validator_index: 20,
                    signature: BLSSignature::infinity(),
                },
                1,
            ),
            (
                SyncCommitteeMessage {
                    slot,
                    beacon_block_root: root,
                    validator_index: 30,
                    signature: BLSSignature::infinity(),
                },
                2,
            ),
        ];

        // Aggregate the messages
        pool.aggregate_messages(slot, root, subcommittee_index, messages);

        // Get the aggregated contributions
        let contributions = pool.get_sync_committee_contributions(slot, root, subcommittee_index);

        assert!(
            !contributions.is_empty(),
            "should have at least one contribution"
        );

        // Find the contribution with the most bits set
        let best = contributions
            .iter()
            .max_by_key(|c| c.aggregation_bits.iter().filter(|b| *b).count())
            .unwrap();

        // Verify bits 0, 1, 2 are set
        assert!(best.aggregation_bits.get(0).unwrap());
        assert!(best.aggregation_bits.get(1).unwrap());
        assert!(best.aggregation_bits.get(2).unwrap());
    }

    #[test]
    fn test_aggregate_messages_avoids_duplicates() {
        let pool = SyncCommitteePool::default();

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        let message = SyncCommitteeMessage {
            slot,
            beacon_block_root: root,
            validator_index: 10,
            signature: BLSSignature::infinity(),
        };

        // Aggregate the same message twice with the same index
        pool.aggregate_messages(slot, root, subcommittee_index, vec![(message.clone(), 0)]);
        pool.aggregate_messages(slot, root, subcommittee_index, vec![(message, 0)]);

        let contributions = pool.get_sync_committee_contributions(slot, root, subcommittee_index);

        // Should still have one contribution with bit 0 set only once
        assert_eq!(contributions.len(), 1);
        assert!(contributions[0].aggregation_bits.get(0).unwrap());

        let count = contributions[0]
            .aggregation_bits
            .iter()
            .filter(|b| *b)
            .count();
        assert_eq!(count, 1, "should have exactly 1 bit set, not duplicated");
    }

    #[test]
    fn test_insert_sync_committee_message() {
        let pool = SyncCommitteePool::default();

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        let message = SyncCommitteeMessage {
            slot,
            beacon_block_root: root,
            validator_index: 42,
            signature: BLSSignature::infinity(),
        };

        // Insert message
        pool.insert_sync_committee_message(message.clone(), subcommittee_index);

        // Verify message was stored
        let messages = pool.messages.read();
        let key = SyncCommitteeDataKey {
            slot,
            beacon_block_root: root,
            subcommittee_index,
        };
        let stored = messages.get(&key).expect("should have messages for key");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].validator_index, 42);
    }

    #[test]
    fn test_insert_sync_committee_message_capacity_limit() {
        let pool = SyncCommitteePool::default();
        let max_capacity = pool.max_contributions_per_key;

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Insert max_capacity messages
        for i in 0..max_capacity {
            let message = SyncCommitteeMessage {
                slot,
                beacon_block_root: root,
                validator_index: i as u64,
                signature: BLSSignature::infinity(),
            };
            pool.insert_sync_committee_message(message, subcommittee_index);
        }

        // Verify we hit the limit
        let messages = pool.messages.read();
        let key = SyncCommitteeDataKey {
            slot,
            beacon_block_root: root,
            subcommittee_index,
        };
        let stored = messages.get(&key).expect("should have messages for key");
        assert_eq!(stored.len(), max_capacity);

        // Try to insert one more - should be ignored
        let extra_message = SyncCommitteeMessage {
            slot,
            beacon_block_root: root,
            validator_index: 999,
            signature: BLSSignature::infinity(),
        };
        drop(messages);
        pool.insert_sync_committee_message(extra_message, subcommittee_index);

        let messages = pool.messages.read();
        let stored = messages.get(&key).expect("should have messages for key");
        assert_eq!(stored.len(), max_capacity, "should not exceed capacity");
    }

    #[test]
    fn test_get_sync_committee_contributions_empty() {
        let pool = SyncCommitteePool::default();

        let contributions = pool.get_sync_committee_contributions(100, B256::from([1u8; 32]), 1);

        assert!(
            contributions.is_empty(),
            "should return empty vec when no contributions exist"
        );
    }

    #[test]
    fn test_get_best_sync_committee_contribution_returns_highest_participation() {
        let pool = SyncCommitteePool::default();

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Create contributions with different participation levels
        let messages = vec![
            (
                SyncCommitteeMessage {
                    slot,
                    beacon_block_root: root,
                    validator_index: 1,
                    signature: BLSSignature::infinity(),
                },
                0,
            ),
            (
                SyncCommitteeMessage {
                    slot,
                    beacon_block_root: root,
                    validator_index: 2,
                    signature: BLSSignature::infinity(),
                },
                1,
            ),
            (
                SyncCommitteeMessage {
                    slot,
                    beacon_block_root: root,
                    validator_index: 3,
                    signature: BLSSignature::infinity(),
                },
                2,
            ),
        ];

        pool.aggregate_messages(slot, root, subcommittee_index, messages);

        let best = pool
            .get_best_sync_committee_contribution(slot, root, subcommittee_index)
            .expect("should return best contribution");

        let count = best.aggregation_bits.iter().filter(|b| *b).count();
        assert_eq!(count, 3, "best contribution should have 3 bits set");
    }

    #[test]
    fn test_get_best_sync_committee_contribution_returns_none_when_empty() {
        let pool = SyncCommitteePool::default();

        let best = pool.get_best_sync_committee_contribution(100, B256::from([1u8; 32]), 1);

        assert!(
            best.is_none(),
            "should return None when no contributions exist"
        );
    }

    #[test]
    fn test_clean_sync_committee_contributions() {
        let pool = SyncCommitteePool::default();

        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Insert contributions for multiple slots
        for slot in [100, 101, 102, 103] {
            pool.aggregate_messages(
                slot,
                root,
                subcommittee_index,
                vec![(
                    SyncCommitteeMessage {
                        slot,
                        beacon_block_root: root,
                        validator_index: 42,
                        signature: BLSSignature::infinity(),
                    },
                    0,
                )],
            );
        }

        // Clean at slot 102 - should keep slots 101 and 102 (current_slot - 1 and current_slot)
        pool.clean_sync_committee_contributions(102);

        // Verify old slots are removed
        assert!(
            pool.get_sync_committee_contributions(100, root, subcommittee_index)
                .is_empty(),
            "slot 100 should be cleaned"
        );

        // Verify recent slots are kept
        assert!(
            !pool
                .get_sync_committee_contributions(101, root, subcommittee_index)
                .is_empty(),
            "slot 101 should be kept"
        );
        assert!(
            !pool
                .get_sync_committee_contributions(102, root, subcommittee_index)
                .is_empty(),
            "slot 102 should be kept"
        );
        assert!(
            !pool
                .get_sync_committee_contributions(103, root, subcommittee_index)
                .is_empty(),
            "slot 103 should be kept (future slot)"
        );
    }

    #[test]
    fn test_clean_sync_committee_messages() {
        let pool = SyncCommitteePool::default();

        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Insert messages for multiple slots
        for slot in [100, 101, 102, 103] {
            let message = SyncCommitteeMessage {
                slot,
                beacon_block_root: root,
                validator_index: 42,
                signature: BLSSignature::infinity(),
            };
            pool.insert_sync_committee_message(message, subcommittee_index);
        }

        // Clean at slot 102 - should keep slots 101 and 102 (current_slot - 1 and current_slot)
        pool.clean_sync_committee_messages(102);

        let messages = pool.messages.read();

        // Verify old slots are removed
        let key_100 = SyncCommitteeDataKey {
            slot: 100,
            beacon_block_root: root,
            subcommittee_index,
        };
        assert!(
            !messages.contains_key(&key_100),
            "slot 100 should be cleaned"
        );

        // Verify recent slots are kept
        let key_101 = SyncCommitteeDataKey {
            slot: 101,
            beacon_block_root: root,
            subcommittee_index,
        };
        assert!(messages.contains_key(&key_101), "slot 101 should be kept");

        let key_102 = SyncCommitteeDataKey {
            slot: 102,
            beacon_block_root: root,
            subcommittee_index,
        };
        assert!(messages.contains_key(&key_102), "slot 102 should be kept");

        let key_103 = SyncCommitteeDataKey {
            slot: 103,
            beacon_block_root: root,
            subcommittee_index,
        };
        assert!(
            messages.contains_key(&key_103),
            "slot 103 should be kept (future slot)"
        );
    }

    #[test]
    fn test_aggregate_messages_with_multiple_subcommittees() {
        let pool = SyncCommitteePool::default();

        let slot = 100u64;
        let root = B256::from([1u8; 32]);

        // Add messages to different subcommittees
        let messages_subnet_0 = vec![(
            SyncCommitteeMessage {
                slot,
                beacon_block_root: root,
                validator_index: 1,
                signature: BLSSignature::infinity(),
            },
            0,
        )];

        let messages_subnet_1 = vec![(
            SyncCommitteeMessage {
                slot,
                beacon_block_root: root,
                validator_index: 2,
                signature: BLSSignature::infinity(),
            },
            0,
        )];

        pool.aggregate_messages(slot, root, 0, messages_subnet_0);
        pool.aggregate_messages(slot, root, 1, messages_subnet_1);

        // Verify both subcommittees have their own contributions
        let contributions_0 = pool.get_sync_committee_contributions(slot, root, 0);
        let contributions_1 = pool.get_sync_committee_contributions(slot, root, 1);

        assert!(
            !contributions_0.is_empty(),
            "subcommittee 0 should have contributions"
        );
        assert!(
            !contributions_1.is_empty(),
            "subcommittee 1 should have contributions"
        );
    }

    #[test]
    fn test_custom_max_contributions_per_key() {
        // Test with testnet-sized capacity
        let pool = SyncCommitteePool::new(8);
        assert_eq!(pool.max_contributions_per_key, 8);

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Insert 8 messages (should all be accepted)
        for i in 0..8 {
            let message = SyncCommitteeMessage {
                slot,
                beacon_block_root: root,
                validator_index: i,
                signature: BLSSignature::infinity(),
            };
            pool.insert_sync_committee_message(message, subcommittee_index);
        }

        let messages = pool.messages.read();
        let key = SyncCommitteeDataKey {
            slot,
            beacon_block_root: root,
            subcommittee_index,
        };
        let stored = messages.get(&key).expect("should have messages");
        assert_eq!(stored.len(), 8);

        // Try to insert 9th message - should be ignored
        drop(messages);
        let extra_message = SyncCommitteeMessage {
            slot,
            beacon_block_root: root,
            validator_index: 999,
            signature: BLSSignature::infinity(),
        };
        pool.insert_sync_committee_message(extra_message, subcommittee_index);

        let messages = pool.messages.read();
        let stored = messages.get(&key).expect("should have messages");
        assert_eq!(stored.len(), 8, "should not exceed custom capacity of 8");
    }

    #[test]
    fn test_aggregate_messages_creates_initial_contribution() {
        let pool = SyncCommitteePool::default();

        let slot = 100u64;
        let root = B256::from([1u8; 32]);
        let subcommittee_index = 1u64;

        // Verify pool is initially empty
        assert!(
            pool.get_sync_committee_contributions(slot, root, subcommittee_index)
                .is_empty()
        );

        // Aggregate with empty iterator - should create initial contribution
        pool.aggregate_messages(slot, root, subcommittee_index, vec![]);

        // Should now have one contribution (the initial empty one)
        let contributions = pool.get_sync_committee_contributions(slot, root, subcommittee_index);
        assert_eq!(contributions.len(), 1);
        assert_eq!(
            contributions[0]
                .aggregation_bits
                .iter()
                .filter(|b| *b)
                .count(),
            0,
            "initial contribution should have no bits set"
        );
    }

    #[test]
    fn test_bit_not_set_on_aggregation_failure() {
        let pool = SyncCommitteePool::default();

        let slot = 200u64;
        let root = B256::from([2u8; 32]);
        let subcommittee_index = 0u64;

        // Construct an invalid/corrupted signature that should fail conversion/validation.
        let bad_sig = BLSSignature {
            inner: FixedVector::<u8, U96>::from(vec![0u8; 96]),
        };

        let message = SyncCommitteeMessage {
            slot,
            beacon_block_root: root,
            validator_index: 7,
            signature: bad_sig,
        };

        // Aggregate a single bad message at index 0
        pool.aggregate_messages(slot, root, subcommittee_index, vec![(message, 0)]);

        let contributions = pool.get_sync_committee_contributions(slot, root, subcommittee_index);
        assert!(
            !contributions.is_empty(),
            "there should be at least the initial contribution"
        );

        // The initial contribution is created before processing messages.
        // Ensure its bit 0 is NOT set since aggregation failed.
        let initial = &contributions[0];
        assert!(
            !initial.aggregation_bits.get(0).unwrap_or(false),
            "bit must not be set in the pre-existing contribution when aggregation fails"
        );
    }
}
