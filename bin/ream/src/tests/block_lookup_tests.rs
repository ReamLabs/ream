use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::B256;
use ream_chain_beacon::beacon_chain::{BeaconChain, BlockImportEvent};
use ream_consensus_beacon::{
    data_column_sidecar::{
        ColumnIdentifier, DataColumnSidecar, get_data_column_sidecars_from_block,
    },
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_block_body::BeaconBlockBody,
        beacon_state::BeaconState,
    },
    matrix_entry::{compute_cells_and_kzg_proofs, das_context},
};
use ream_consensus_misc::checkpoint::Checkpoint;
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice_beacon::data_availability::{
    AvailabilityEntryStatus, DataAvailabilityChecker,
};
use ream_mock_execution_engine::block_generator::{
    genesis_execution_payload, sample_blob_and_commitment,
};
use ream_network_manager::{
    block_lookup::{
        BlockLookupConfig, BlockLookupCoordinator, StageOutcome, apply_block_import_event,
        apply_coordinator_update, execute_coordinator_action, stage_deferred_item,
    },
    gossipsub::handle::{Message, MessageAcceptance, handle_gossipsub_message},
    p2p_sender::P2PSender,
};
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::beacon::topics::{GossipTopic, GossipTopicKind},
    network::beacon::channel::{P2PMessage, P2PRequest},
};
use ream_storage::{
    cache::{AddressSlotIdentifier, BeaconCacheDB},
    tables::{
        field::REDBField,
        table::{CustomTable, REDBTable},
    },
};
use ream_sync_committee_pool::SyncCommitteePool;
use ream_validator_beacon::{block::sign_beacon_block, randao::sign_randao_reveal};
use serial_test::serial;
use ssz::Encode;
use ssz_types::VariableList;
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};
use tree_hash::TreeHash;

use super::{
    beacon_e2e_dev_spec, beacon_e2e_public_keys, build_dev_genesis, create_beacon_test_node_db,
    indexed_private_key, initialize_beacon_e2e_genesis_root, initialize_beacon_e2e_network_spec,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(60);

struct BlobBlockFixture {
    signed_block: SignedBeaconBlock,
    post_state: Option<BeaconState>,
    column: DataColumnSidecar,
}

struct GossipLookupHarness {
    beacon_chain: Arc<BeaconChain>,
    cached_db: Arc<BeaconCacheDB>,
    p2p_sender: P2PSender,
    p2p_receiver: mpsc::UnboundedReceiver<P2PMessage>,
    import_receiver: broadcast::Receiver<BlockImportEvent>,
    coordinator: BlockLookupCoordinator,
}

impl GossipLookupHarness {
    async fn new(test_name: &str) -> (Self, BeaconState, SignedBeaconBlock) {
        initialize_beacon_e2e_network_spec(beacon_e2e_dev_spec());
        let public_keys = beacon_e2e_public_keys();
        let (genesis_state, genesis_block) = build_dev_genesis(&public_keys);
        let ream_db = create_beacon_test_node_db(test_name, 1);
        let genesis_validators_root =
            super::seed_beacon_test_db(&ream_db, genesis_state.clone(), &genesis_block);
        initialize_beacon_e2e_genesis_root(genesis_validators_root);

        let cached_db = Arc::new(BeaconCacheDB::new());
        let beacon_db = ream_db
            .init_beacon_db()
            .expect("beacon DB should reopen")
            .with_cache(cached_db.clone());
        let beacon_chain = Arc::new(
            BeaconChain::new(
                beacon_db,
                Arc::new(OperationPool::default()),
                Arc::new(SyncCommitteePool::default()),
                None,
                None,
            )
            .force_data_availability_checks(),
        );
        {
            let mut store = beacon_chain.store.lock().await;
            store.data_availability_checker = DataAvailabilityChecker::new(HashSet::from([0]));

            // Poison the storage-level "latest state" independently of fork choice. The legacy
            // validation path used this state and would reject every correctly signed fixture
            // below because its validator public keys deliberately do not match.
            let mut unrelated_latest_state = genesis_state.clone();
            unrelated_latest_state.slot = 64;
            let unrelated_public_key =
                super::public_key_from_private_key(indexed_private_key(10_000));
            assert!(
                !public_keys.contains(&unrelated_public_key),
                "the poisoned latest-state key must not belong to any fixture validator"
            );
            for validator in unrelated_latest_state.validators.iter_mut() {
                validator.public_key = unrelated_public_key.clone();
            }
            let unrelated_root = B256::repeat_byte(0xee);
            store
                .db
                .state_provider()
                .insert(unrelated_root, unrelated_latest_state)
                .expect("unrelated latest state should insert");
            store
                .db
                .slot_index_provider()
                .insert(64, unrelated_root)
                .expect("unrelated latest slot should insert");

            let head_root = store.get_head().expect("genesis should remain the head");
            let head_state = store
                .db
                .state_provider()
                .get(head_root)
                .expect("head-state lookup should succeed")
                .expect("head state should exist");
            let latest_state = store
                .db
                .get_latest_state()
                .expect("latest state should exist");
            assert_eq!(head_state.slot, 0);
            assert_eq!(latest_state.slot, 64);
            assert_ne!(
                latest_state.validators[0].public_key, head_state.validators[0].public_key,
                "the fixture must detect accidental get_latest_state validation"
            );
        }

        let import_receiver = beacon_chain.subscribe_block_imports();
        let (p2p_tx, p2p_receiver) = mpsc::unbounded_channel();
        (
            Self {
                beacon_chain,
                cached_db,
                p2p_sender: P2PSender(p2p_tx),
                p2p_receiver,
                import_receiver,
                coordinator: BlockLookupCoordinator::new(
                    BlockLookupConfig::for_data_column_retention(
                        ream_network_spec::networks::beacon_network_spec()
                            .min_epochs_for_data_column_sidecars_requests,
                    ),
                ),
            },
            genesis_state,
            genesis_block,
        )
    }

    async fn wait_for_slot(&self, target_slot: u64) {
        timeout(Duration::from_secs(15), async {
            loop {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time should follow the Unix epoch")
                    .as_secs();
                self.beacon_chain
                    .process_tick(now)
                    .await
                    .expect("test tick should process");
                let current_slot = self
                    .beacon_chain
                    .store
                    .lock()
                    .await
                    .get_current_slot()
                    .expect("current slot should be available");
                if current_slot >= target_slot {
                    return;
                }
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("test genesis should reach the target slot");
    }

    async fn deliver_block(
        &mut self,
        signed_block: &SignedBeaconBlock,
    ) -> (MessageAcceptance, Vec<StageOutcome>) {
        self.deliver(GossipTopicKind::BeaconBlock, signed_block.as_ssz_bytes())
            .await
    }

    async fn deliver_column(
        &mut self,
        column: &DataColumnSidecar,
    ) -> (MessageAcceptance, Vec<StageOutcome>) {
        self.deliver(
            GossipTopicKind::DataColumnSidecar(column.compute_subnet()),
            column.as_ssz_bytes(),
        )
        .await
    }

    async fn deliver(
        &mut self,
        kind: GossipTopicKind,
        data: Vec<u8>,
    ) -> (MessageAcceptance, Vec<StageOutcome>) {
        let topic = GossipTopic {
            fork: ream_network_spec::networks::beacon_network_spec().fork_digest(
                ream_consensus_misc::constants::beacon::FULU_FORK_EPOCH,
                ream_consensus_misc::constants::beacon::genesis_validators_root(),
            ),
            kind,
        };
        let mut deferred_item = None;
        let acceptance = handle_gossipsub_message(
            Message {
                source: None,
                data,
                sequence_number: None,
                topic: topic.into(),
            },
            &self.beacon_chain,
            &self.cached_db,
            &self.p2p_sender,
            &mut deferred_item,
        )
        .await;

        let mut outcomes = Vec::new();
        if let Some(item) = deferred_item {
            let current_slot = self
                .beacon_chain
                .store
                .lock()
                .await
                .get_current_slot()
                .expect("current slot should be available");
            outcomes.push(stage_deferred_item(
                &mut self.coordinator,
                item,
                current_slot,
            ));
        }
        (acceptance, outcomes)
    }

    async fn resume_until_idle(&mut self) {
        for _ in 0..64 {
            let mut made_progress = false;
            loop {
                match self.import_receiver.try_recv() {
                    Ok(event) => {
                        apply_block_import_event(&mut self.coordinator, event);
                        made_progress = true;
                    }
                    Err(broadcast::error::TryRecvError::Empty)
                    | Err(broadcast::error::TryRecvError::Closed) => break,
                    Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                        panic!("test import receiver lagged by {skipped} events")
                    }
                }
            }

            if let Some(action) = self.coordinator.next_action() {
                made_progress = true;
                let update = execute_coordinator_action(action, &self.beacon_chain).await;
                apply_coordinator_update(&mut self.coordinator, update);
            }

            if !made_progress {
                return;
            }
        }
        panic!("coordinator did not become idle within the bounded test work loop");
    }

    fn assert_no_blocks_by_root_request(&mut self) {
        while let Ok(message) = self.p2p_receiver.try_recv() {
            assert!(
                !matches!(message, P2PMessage::Request(P2PRequest::BlockRoots { .. })),
                "a locally DA-pending parent must not trigger BlocksByRoot"
            );
        }
    }

    async fn assert_imported(&self, block_root: B256) {
        let store = self.beacon_chain.store.lock().await;
        assert!(
            store
                .db
                .block_provider()
                .get(block_root)
                .expect("block lookup should succeed")
                .is_some(),
            "block {block_root} should be imported"
        );
        assert!(
            store
                .db
                .state_provider()
                .get(block_root)
                .expect("state lookup should succeed")
                .is_some(),
            "state {block_root} should be imported"
        );
    }
}

async fn build_blob_block(
    parent_state: &BeaconState,
    parent_block: &SignedBeaconBlock,
    slot: u64,
    proposer_override: Option<u64>,
    blob_seed: u8,
) -> BlobBlockFixture {
    let parent_root = parent_block.message.tree_hash_root();
    let mut state_at_slot = parent_state.clone();
    state_at_slot
        .process_slots(slot)
        .expect("fixture state should advance");
    let expected_proposer = state_at_slot
        .get_beacon_proposer_index(None)
        .expect("fixture should have a proposer");
    let proposer_index = proposer_override.unwrap_or(expected_proposer);
    let private_key = ream_bls::PrivateKey {
        inner: indexed_private_key(proposer_index as usize),
    };
    let (blob, commitment) =
        sample_blob_and_commitment(blob_seed).expect("sample blob should be valid");

    let mut execution_payload = genesis_execution_payload(
        state_at_slot.latest_execution_payload_header.block_hash,
        state_at_slot.compute_timestamp_at_slot(slot),
    );
    execution_payload.prev_randao = state_at_slot.get_randao_mix(state_at_slot.get_current_epoch());
    execution_payload.block_number = state_at_slot.latest_execution_payload_header.block_number + 1;
    execution_payload.block_hash = execution_payload
        .to_execution_header(parent_root, &[])
        .hash_slow();

    let mut body = BeaconBlockBody {
        randao_reveal: sign_randao_reveal(slot, &private_key).expect("RANDAO reveal should sign"),
        eth1_data: state_at_slot.eth1_data.clone(),
        execution_payload,
        blob_kzg_commitments: VariableList::new(vec![commitment])
            .expect("one commitment should fit"),
        ..Default::default()
    };
    body.sync_aggregate.sync_committee_signature = ream_bls::BLSSignature::infinity();
    let mut block = BeaconBlock {
        slot,
        proposer_index,
        parent_root,
        state_root: B256::ZERO,
        body,
    };

    let post_state = if proposer_override.is_none() {
        let mut post_state = state_at_slot;
        post_state
            .process_block(&block, &Option::<ExecutionEngine>::None)
            .await
            .expect("valid fixture block should transition");
        block.state_root = post_state.tree_hash_root();
        Some(post_state)
    } else {
        None
    };
    let signed_block = sign_beacon_block(block, &private_key).expect("fixture block should sign");
    let cells_and_proofs =
        compute_cells_and_kzg_proofs(&blob, das_context()).expect("blob should encode to cells");
    let sidecars = get_data_column_sidecars_from_block(&signed_block, vec![cells_and_proofs])
        .expect("fixture sidecars should build");
    assert_eq!(signed_block.message.body.blob_kzg_commitments.len(), 1);
    assert!(!sidecars[0].column.is_empty());

    BlobBlockFixture {
        signed_block,
        post_state,
        column: sidecars[0].clone(),
    }
}

fn corrupt_fixture_state_root(fixture: &mut BlobBlockFixture, blob_seed: u8) {
    fixture.signed_block.message.state_root = B256::repeat_byte(0xa5);
    let proposer_index = fixture.signed_block.message.proposer_index;
    let private_key = ream_bls::PrivateKey {
        inner: indexed_private_key(proposer_index as usize),
    };
    fixture.signed_block = sign_beacon_block(fixture.signed_block.message.clone(), &private_key)
        .expect("corrupted fixture should still sign");

    let (blob, commitment) =
        sample_blob_and_commitment(blob_seed).expect("sample blob should be reproducible");
    assert_eq!(
        fixture.signed_block.message.body.blob_kzg_commitments[0],
        commitment
    );
    let cells_and_proofs =
        compute_cells_and_kzg_proofs(&blob, das_context()).expect("blob should encode to cells");
    fixture.column =
        get_data_column_sidecars_from_block(&fixture.signed_block, vec![cells_and_proofs])
            .expect("corrupted fixture sidecar should build")[0]
            .clone();
}

#[tokio::test]
#[serial]
async fn test_pending_parent_block_imports_after_parent_becomes_available() {
    let (mut harness, genesis_state, genesis_block) =
        GossipLookupHarness::new("pending_parent_lookup").await;
    harness.wait_for_slot(4).await;

    let parent = build_blob_block(&genesis_state, &genesis_block, 1, None, 1).await;
    let child = build_blob_block(
        parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state"),
        &parent.signed_block,
        2,
        None,
        2,
    )
    .await;
    let expected_sibling_proposer = {
        let mut state = parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state")
            .clone();
        state.process_slots(3).expect("state should advance");
        state
            .get_beacon_proposer_index(None)
            .expect("slot should have proposer")
    };
    let invalid_sibling_proposer = (expected_sibling_proposer + 1)
        % u64::try_from(genesis_state.validators.len()).expect("validator count should fit");
    let invalid_sibling = build_blob_block(
        parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state"),
        &parent.signed_block,
        3,
        Some(invalid_sibling_proposer),
        3,
    )
    .await;
    let late_child = build_blob_block(
        parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state"),
        &parent.signed_block,
        3,
        None,
        5,
    )
    .await;
    let mut failed_child = build_blob_block(
        parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state"),
        &parent.signed_block,
        4,
        None,
        6,
    )
    .await;
    corrupt_fixture_state_root(&mut failed_child, 6);

    let parent_root = parent.signed_block.message.tree_hash_root();
    let child_root = child.signed_block.message.tree_hash_root();
    let sibling_root = invalid_sibling.signed_block.message.tree_hash_root();
    let late_child_root = late_child.signed_block.message.tree_hash_root();
    let failed_child_root = failed_child.signed_block.message.tree_hash_root();

    let (acceptance, staged) = harness.deliver_block(&parent.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Accept));
    assert!(staged.is_empty());
    assert_eq!(
        harness
            .beacon_chain
            .store
            .lock()
            .await
            .data_availability_checker
            .status(&parent_root),
        AvailabilityEntryStatus::PendingBlock
    );

    let (poisoned_head_root, poisoned_proposer_index, original_head_public_key) = {
        // The exact pending-parent registry, not the competing head registry, must authorize both
        // the child block and its column after the mandatory pre-parent signature check.
        let store = harness.beacon_chain.store.lock().await;
        let head_root = store.get_head().expect("head should be available");
        assert_ne!(head_root, parent_root);
        let mut head_state = store
            .db
            .state_provider()
            .get(head_root)
            .expect("head state lookup should succeed")
            .expect("head state should exist");
        let proposer_index = child.signed_block.message.proposer_index as usize;
        let unrelated_public_key = super::public_key_from_private_key(indexed_private_key(20_000));
        let original_head_public_key = head_state.validators[proposer_index].public_key.clone();
        assert_ne!(original_head_public_key, unrelated_public_key);
        head_state.validators[proposer_index].public_key = unrelated_public_key;
        store
            .db
            .state_provider()
            .insert(head_root, head_state)
            .expect("poisoned head state should insert");
        (head_root, proposer_index, original_head_public_key)
    };

    let (acceptance, staged) = harness.deliver_block(&child.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Ignore));
    assert!(matches!(staged.as_slice(), [StageOutcome::Inserted]));
    let child_proposer = parent
        .post_state
        .as_ref()
        .expect("valid parent should have post-state")
        .validators
        .get(child.signed_block.message.proposer_index as usize)
        .expect("child proposer should exist");
    assert!(
        harness
            .cached_db
            .seen_proposer_signature
            .read()
            .await
            .contains(&AddressSlotIdentifier {
                address: child_proposer.public_key.clone(),
                slot: child.signed_block.message.slot,
            }),
        "a fully validated deferred child must enter the seen cache at arrival"
    );
    let staged_count = harness.coordinator.staged_block_count();
    let (duplicate_acceptance, duplicate) = harness.deliver_block(&child.signed_block).await;
    assert!(matches!(duplicate_acceptance, MessageAcceptance::Ignore));
    assert!(duplicate.is_empty());
    assert_eq!(harness.coordinator.staged_block_count(), staged_count);

    let (acceptance, staged) = harness.deliver_block(&invalid_sibling.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Reject));
    assert!(staged.is_empty());
    assert_eq!(
        harness.coordinator.children(&parent_root),
        vec![child_root],
        "a wrong-proposer child must not enter the coordinator"
    );
    let (acceptance, staged) = harness.deliver_column(&invalid_sibling.column).await;
    assert!(matches!(acceptance, MessageAcceptance::Reject));
    assert!(staged.is_empty());
    assert!(
        !harness
            .coordinator
            .contains_column(&ColumnIdentifier::new(sibling_root, 0)),
        "a wrong-proposer column must be rejected before quarantine"
    );

    let (acceptance, staged) = harness.deliver_block(&failed_child.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Ignore));
    assert!(matches!(staged.as_slice(), [StageOutcome::Inserted]));
    assert_eq!(
        harness.coordinator.children(&parent_root),
        vec![child_root, failed_child_root],
        "multiple valid direct children must coexist under one DA-pending parent"
    );

    for column in [&child.column, &failed_child.column] {
        let (acceptance, staged) = harness.deliver_column(column).await;
        assert!(matches!(acceptance, MessageAcceptance::Ignore));
        assert!(matches!(staged.as_slice(), [StageOutcome::Inserted]));
    }
    let (acceptance, staged) = harness.deliver_column(&late_child.column).await;
    assert!(matches!(acceptance, MessageAcceptance::Ignore));
    assert!(matches!(staged.as_slice(), [StageOutcome::Inserted]));
    assert!(
        harness
            .coordinator
            .contains_column(&ColumnIdentifier::new(late_child_root, 0))
    );

    {
        // Restore the canonical parent state before validating the parent's own column. The
        // temporary mutation above exists only to force the competing-head fallback paths.
        let store = harness.beacon_chain.store.lock().await;
        let mut head_state = store
            .db
            .state_provider()
            .get(poisoned_head_root)
            .expect("head state lookup should succeed")
            .expect("head state should exist");
        head_state.validators[poisoned_proposer_index].public_key = original_head_public_key;
        store
            .db
            .state_provider()
            .insert(poisoned_head_root, head_state)
            .expect("restored head state should insert");
    }

    harness.assert_no_blocks_by_root_request();
    let (acceptance, staged) = timeout(TEST_TIMEOUT, harness.deliver_column(&parent.column))
        .await
        .expect("parent DA completion deadlocked while children were queued");
    assert!(matches!(acceptance, MessageAcceptance::Accept));
    assert!(staged.is_empty());

    // This block was never staged: only its column arrived before the parent import. Its typed
    // PendingAvailability outcome must still wake the existing quarantine.
    let (acceptance, staged) = harness.deliver_block(&late_child.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Accept));
    assert!(staged.is_empty());

    timeout(TEST_TIMEOUT, harness.resume_until_idle())
        .await
        .expect("deferred import deadlocked, likely while holding the Store guard");

    harness.assert_imported(parent_root).await;
    harness.assert_imported(child_root).await;
    harness.assert_imported(late_child_root).await;
    {
        let store = harness.beacon_chain.store.lock().await;
        let child_weight = store
            .get_weight(child_root)
            .expect("child branch weight should be available");
        let late_child_weight = store
            .get_weight(late_child_root)
            .expect("late-child branch weight should be available");
        let expected_head = if (child_weight, child_root) > (late_child_weight, late_child_root) {
            child_root
        } else {
            late_child_root
        };
        assert_eq!(
            store
                .get_head()
                .expect("fork-choice head should be available"),
            expected_head,
            "all imported direct children must participate in fork choice"
        );
        assert!(
            store
                .db
                .block_provider()
                .get(sibling_root)
                .expect("sibling lookup should succeed")
                .is_none(),
            "wrong-proposer sibling must not import"
        );
        assert!(
            store
                .db
                .column_sidecars_provider()
                .get(ColumnIdentifier::new(sibling_root, 0))
                .expect("sibling column lookup should succeed")
                .is_none(),
            "wrong-proposer sibling's column must not enter served storage"
        );
        assert!(
            store
                .db
                .block_provider()
                .get(failed_child_root)
                .expect("failed child lookup should succeed")
                .is_none(),
            "a staged child that fails full import must be dropped"
        );
        assert!(
            store
                .db
                .column_sidecars_provider()
                .get(ColumnIdentifier::new(failed_child_root, 0))
                .expect("failed child column lookup should succeed")
                .is_none(),
            "a failed child's quarantined column must not enter served storage"
        );
    }
    assert_eq!(harness.coordinator.staged_entry_count(), 0);
    assert_eq!(harness.coordinator.pending_action_count(), 0);
    harness.assert_no_blocks_by_root_request();
}

#[tokio::test]
#[serial]
async fn test_staged_child_is_dropped_if_finality_advances_past_it() {
    let (mut harness, genesis_state, genesis_block) =
        GossipLookupHarness::new("pending_child_finality_advance").await;
    harness.wait_for_slot(2).await;

    let parent = build_blob_block(&genesis_state, &genesis_block, 1, None, 21).await;
    let child = build_blob_block(
        parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state"),
        &parent.signed_block,
        2,
        None,
        22,
    )
    .await;
    let parent_root = parent.signed_block.message.tree_hash_root();
    let child_root = child.signed_block.message.tree_hash_root();

    let (acceptance, staged) = harness.deliver_block(&parent.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Accept));
    assert!(staged.is_empty());
    let (acceptance, staged) = harness.deliver_block(&child.signed_block).await;
    assert!(matches!(acceptance, MessageAcceptance::Ignore));
    assert!(matches!(staged.as_slice(), [StageOutcome::Inserted]));
    let (acceptance, staged) = harness.deliver_column(&child.column).await;
    assert!(matches!(acceptance, MessageAcceptance::Ignore));
    assert!(matches!(staged.as_slice(), [StageOutcome::Inserted]));

    let (acceptance, staged) = harness.deliver_column(&parent.column).await;
    assert!(matches!(acceptance, MessageAcceptance::Accept));
    assert!(staged.is_empty());
    harness.assert_imported(parent_root).await;

    {
        let store = harness.beacon_chain.store.lock().await;
        let current = store
            .db
            .finalized_checkpoint_provider()
            .get()
            .expect("finalized checkpoint should exist");
        store
            .db
            .finalized_checkpoint_provider()
            .insert(Checkpoint {
                epoch: 1,
                root: current.root,
            })
            .expect("test finality should advance");
    }

    harness.coordinator.parent_imported(parent_root);
    let action = harness
        .coordinator
        .next_action()
        .expect("parent completion should queue the staged child");
    let update = execute_coordinator_action(action, &harness.beacon_chain).await;
    apply_coordinator_update(&mut harness.coordinator, update);

    assert!(!harness.coordinator.contains_block(&child_root));
    assert!(
        !harness
            .coordinator
            .contains_column(&ColumnIdentifier::new(child_root, 0)),
        "finality rejection must drop the child's quarantined columns"
    );
    let store = harness.beacon_chain.store.lock().await;
    assert!(
        store
            .db
            .block_provider()
            .get(child_root)
            .expect("child lookup should succeed")
            .is_none(),
        "a child passed by finality while staged must not import"
    );
}

#[tokio::test]
#[serial]
async fn test_finality_advance_before_column_release_does_not_import_pending_child() {
    let (mut harness, genesis_state, genesis_block) =
        GossipLookupHarness::new("pending_child_column_finality_advance").await;
    harness.wait_for_slot(2).await;

    let parent = build_blob_block(&genesis_state, &genesis_block, 1, None, 31).await;
    let child = build_blob_block(
        parent
            .post_state
            .as_ref()
            .expect("valid parent should have post-state"),
        &parent.signed_block,
        2,
        None,
        32,
    )
    .await;
    let parent_root = parent.signed_block.message.tree_hash_root();
    let child_root = child.signed_block.message.tree_hash_root();
    let child_column = ColumnIdentifier::new(child_root, 0);

    assert!(matches!(
        harness.deliver_block(&parent.signed_block).await.0,
        MessageAcceptance::Accept
    ));
    assert!(matches!(
        harness.deliver_block(&child.signed_block).await.0,
        MessageAcceptance::Ignore
    ));
    assert!(matches!(
        harness.deliver_column(&child.column).await.0,
        MessageAcceptance::Ignore
    ));
    assert!(matches!(
        harness.deliver_column(&parent.column).await.0,
        MessageAcceptance::Accept
    ));
    harness.assert_imported(parent_root).await;

    harness.coordinator.parent_imported(parent_root);
    let block_action = harness
        .coordinator
        .next_action()
        .expect("parent completion should queue the child block");
    let block_update = execute_coordinator_action(block_action, &harness.beacon_chain).await;
    apply_coordinator_update(&mut harness.coordinator, block_update);
    assert_eq!(
        harness
            .beacon_chain
            .store
            .lock()
            .await
            .data_availability_checker
            .status(&child_root),
        AvailabilityEntryStatus::PendingBlock
    );

    {
        let store = harness.beacon_chain.store.lock().await;
        let current = store
            .db
            .finalized_checkpoint_provider()
            .get()
            .expect("finalized checkpoint should exist");
        store
            .db
            .finalized_checkpoint_provider()
            .insert(Checkpoint {
                epoch: 1,
                root: current.root,
            })
            .expect("test finality should advance");
    }

    let column_action = harness
        .coordinator
        .next_action()
        .expect("pending child should queue its quarantined column");
    let column_update = execute_coordinator_action(column_action, &harness.beacon_chain).await;
    apply_coordinator_update(&mut harness.coordinator, column_update);

    assert!(!harness.coordinator.contains_column(&child_column));
    let store = harness.beacon_chain.store.lock().await;
    assert!(
        store
            .db
            .column_sidecars_provider()
            .get(child_column)
            .expect("child column lookup should succeed")
            .is_none(),
        "release validation must run before the column enters served storage"
    );
    assert!(
        store
            .db
            .block_provider()
            .get(child_root)
            .expect("child lookup should succeed")
            .is_none(),
        "finality must not race column completion into importing the pending child"
    );
}
