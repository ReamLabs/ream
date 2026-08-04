#[allow(clippy::unwrap_used)]
mod tests {
    const PATH_TO_TEST_DATA_FOLDER: &str = "./tests";
    use std::{path::PathBuf, str::FromStr, sync::Arc};

    use alloy_primitives::B256;
    use anyhow::anyhow;
    use ream_chain_beacon::beacon_chain::BeaconChain;
    use ream_consensus_beacon::{
        bls_to_execution_change::BLSToExecutionChange,
        electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    };
    use ream_consensus_misc::{checkpoint::Checkpoint, misc::compute_start_slot_at_epoch};
    use ream_network_manager::gossipsub::validate::{
        beacon_block::validate_gossip_beacon_block, result::DependencyValidationResult,
    };
    use ream_network_spec::networks::initialize_test_network_spec;
    use ream_operation_pool::OperationPool;
    use ream_storage::{
        cache::{AddressSlotIdentifier, BeaconCacheDB},
        db::{ReamDB, beacon::BeaconDB},
        tables::{field::REDBField, table::REDBTable},
    };
    use ream_sync_committee_pool::SyncCommitteePool;
    use snap::raw::Decoder;
    use ssz::Decode;
    use tempdir::TempDir;

    const SEPOLIA_GENESIS_TIME: u64 = 1655733600;
    const CURRENT_TIME: u64 = 1770358512;

    pub async fn db_setup() -> (BeaconChain, Arc<BeaconCacheDB>, B256) {
        let (beacon_chain, cached_db, block_root, _, _) = db_setup_with_parent(true).await;
        (beacon_chain, cached_db, block_root)
    }

    pub async fn db_setup_with_parent(
        include_parent: bool,
    ) -> (
        BeaconChain,
        Arc<BeaconCacheDB>,
        B256,
        SignedBeaconBlock,
        BeaconState,
    ) {
        let temp_dir = TempDir::new("ream_gossip_test").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let ream_db = ReamDB::new(temp_path).expect("unable to init Ream Database");
        let cached_db = Arc::new(BeaconCacheDB::default());
        let mut db = ream_db
            .init_beacon_db()
            .unwrap()
            .with_cache(cached_db.clone());

        let ancestor_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/ancestor_9551968.ssz_snappy",
        )
        .unwrap();

        let grandparent_beacon_state = read_ssz_snappy_file::<BeaconState>(
            "./assets/sepolia/states/grandparent_state_9552074.ssz_snappy",
        )
        .unwrap();

        let grandparent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/grandparent_9552074.ssz_snappy",
        )
        .unwrap();

        let parent_beacon_state = read_ssz_snappy_file::<BeaconState>(
            "./assets/sepolia/states/parent_state_9552075.ssz_snappy",
        )
        .unwrap();

        let parent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/parent_9552075.ssz_snappy",
        )
        .unwrap();

        let block_root = parent_beacon_block.message.block_root();
        let grandparent_block_root = grandparent_beacon_block.message.block_root();
        insert_mock_data(
            &mut db,
            ancestor_beacon_block,
            grandparent_block_root,
            block_root,
            grandparent_beacon_state,
            grandparent_beacon_block,
            parent_beacon_block.clone(),
            parent_beacon_state.clone(),
            include_parent,
        )
        .await;

        let operation_pool = OperationPool::default();
        let sync_committee_pool = SyncCommitteePool::default();
        let beacon_chain = BeaconChain::new(
            db,
            operation_pool.into(),
            sync_committee_pool.into(),
            None,
            None,
        );

        (
            beacon_chain,
            cached_db,
            block_root,
            parent_beacon_block,
            parent_beacon_state,
        )
    }

    #[allow(clippy::too_many_arguments, clippy::unwrap_used)]
    pub async fn insert_mock_data(
        db: &mut BeaconDB,
        ancestor_block: SignedBeaconBlock,
        grandparent_block_root: B256,
        block_root: B256,
        grandparent_state: BeaconState,
        grandparent_block: SignedBeaconBlock,
        parent_block: SignedBeaconBlock,
        parent_state: BeaconState,
        include_parent: bool,
    ) {
        let mut head_block = ancestor_block.clone();
        head_block.message.slot = 0;
        head_block.message.parent_root = B256::ZERO;
        let head_root = head_block.message.block_root();
        let genesis_checkpoint = Checkpoint {
            epoch: 0,
            root: head_root,
        };
        db.block_provider().insert(head_root, head_block).unwrap();
        db.state_provider()
            .insert(head_root, grandparent_state.clone())
            .unwrap();
        db.block_provider()
            .insert(ancestor_block.message.block_root(), ancestor_block)
            .unwrap();

        let slot = parent_block.message.slot;
        db.finalized_checkpoint_provider()
            .insert(genesis_checkpoint)
            .unwrap();
        db.justified_checkpoint_provider()
            .insert(genesis_checkpoint)
            .unwrap();
        db.unrealized_justifications_provider()
            .insert(head_root, genesis_checkpoint)
            .unwrap();
        db.block_provider()
            .insert(grandparent_block_root, grandparent_block)
            .unwrap();
        db.state_provider()
            .insert(grandparent_block_root, grandparent_state)
            .unwrap();
        if include_parent {
            db.block_provider()
                .insert(block_root, parent_block)
                .unwrap();
            db.state_provider()
                .insert(block_root, parent_state)
                .unwrap();
            db.slot_index_provider().insert(slot, block_root).unwrap();
        }
        db.genesis_time_provider()
            .insert(SEPOLIA_GENESIS_TIME)
            .unwrap();
        db.time_provider().insert(CURRENT_TIME).unwrap();
    }

    #[tokio::test]
    pub async fn test_validate_beacon_block() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, block_root) = db_setup().await;

        let (latest_state_in_db, latest_block) = {
            let store = beacon_chain.store.lock().await;

            (
                store.db.get_latest_state().unwrap(),
                store.db.block_provider().get(block_root).unwrap().unwrap(),
            )
        };
        assert_eq!(latest_state_in_db.slot, latest_block.message.slot);
        assert_eq!(latest_block.message.slot, 9552075);

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();

        assert_eq!(incoming_beacon_block.message.slot, 9552076);
        assert_eq!(
            incoming_beacon_block.message.block_root(),
            B256::from_str("0x0645b766a8f23559db28d9eec1b5e7997dee78edd176a1d1b209cd9c44d0f1a8")
                .unwrap()
        );

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();

        assert_eq!(result, DependencyValidationResult::Accept);
    }

    #[tokio::test]
    pub async fn test_valid_block_uses_parent_state_not_unrelated_latest_state() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, parent_root) = db_setup().await;
        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();

        {
            let store = beacon_chain.store.lock().await;
            let parent_state = store.db.state_provider().get(parent_root).unwrap().unwrap();
            let proposer_index = incoming_beacon_block.message.proposer_index as usize;
            let alternate_index = (proposer_index + 1) % parent_state.validators.len();
            let alternate_public_key = parent_state.validators[alternate_index].public_key.clone();
            assert_ne!(
                parent_state.validators[proposer_index].public_key,
                alternate_public_key
            );

            let mut advanced_parent_state = parent_state.clone();
            advanced_parent_state
                .process_slots(incoming_beacon_block.message.slot)
                .unwrap();
            assert_eq!(
                advanced_parent_state
                    .get_beacon_proposer_index(None)
                    .unwrap(),
                incoming_beacon_block.message.proposer_index
            );

            let head_root = store.get_head().unwrap();
            assert_ne!(head_root, parent_root);
            let mut head_state = store.db.state_provider().get(head_root).unwrap().unwrap();
            let lookahead_index = (incoming_beacon_block.message.slot
                - compute_start_slot_at_epoch(head_state.get_current_epoch()))
                as usize;
            head_state.proposer_lookahead[lookahead_index] = alternate_index as u64;
            head_state.validators[proposer_index].public_key = alternate_public_key.clone();
            assert_ne!(
                head_state
                    .get_beacon_proposer_index(Some(incoming_beacon_block.message.slot))
                    .unwrap(),
                incoming_beacon_block.message.proposer_index
            );
            assert_ne!(
                head_state.validators[proposer_index].public_key,
                parent_state.validators[proposer_index].public_key,
                "the tentative head signature check must not override the exact parent registry"
            );
            store
                .db
                .state_provider()
                .insert(head_root, head_state)
                .unwrap();

            let mut unrelated_latest_state = parent_state.clone();
            unrelated_latest_state.slot = incoming_beacon_block.message.slot + 64;
            unrelated_latest_state.validators[proposer_index].public_key = alternate_public_key;

            let unrelated_root = B256::repeat_byte(0x42);
            store
                .db
                .state_provider()
                .insert(unrelated_root, unrelated_latest_state)
                .unwrap();
            store
                .db
                .slot_index_provider()
                .insert(incoming_beacon_block.message.slot + 64, unrelated_root)
                .unwrap();

            let latest_state = store.db.get_latest_state().unwrap();
            assert_eq!(latest_state.slot, incoming_beacon_block.message.slot + 64);
            assert_ne!(
                latest_state.validators[proposer_index].public_key,
                parent_state.validators[proposer_index].public_key
            );
        }

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();

        assert_eq!(result, DependencyValidationResult::Accept);
    }

    #[tokio::test]
    pub async fn test_valid_signature_with_unknown_parent_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, parent_root, _, _) = db_setup_with_parent(false).await;
        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();

        assert!(
            matches!(result, DependencyValidationResult::Ignore(reason) if reason.contains("Parent block not found"))
        );
        assert_ne!(parent_root, B256::ZERO);
    }

    #[tokio::test]
    pub async fn test_bad_signature_with_unknown_parent_is_rejected() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _parent_root, _, _) = db_setup_with_parent(false).await;
        let mut incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();
        incoming_beacon_block.signature = Default::default();

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();

        assert!(
            matches!(result, DependencyValidationResult::Reject(reason) if reason.contains("Invalid signature"))
        );
    }

    #[tokio::test]
    pub async fn test_locally_pending_parent_is_classified_as_data_availability() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, parent_root, parent_block, parent_state) =
            db_setup_with_parent(false).await;
        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();

        {
            let mut store = beacon_chain.store.lock().await;
            store
                .data_availability_checker
                .insert_pending(parent_root, parent_block, parent_state);
            // Make the local entry complete without consuming it. Validation must still avoid
            // classifying this parent as unknown and issuing a network lookup.
            for column_index in 0..ream_consensus_beacon::data_column_sidecar::NUMBER_OF_COLUMNS {
                store.data_availability_checker.add_column(
                    parent_root,
                    column_index,
                    incoming_beacon_block.message.slot - 1,
                );
            }
        }

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();

        assert!(
            matches!(result, DependencyValidationResult::ParentUnavailable {
                parent_root: actual_parent_root,
                ..
            } if actual_parent_root == parent_root)
        );
    }

    #[tokio::test]
    pub async fn test_future_slot_block_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let mut incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();
        let future_slot = beacon_chain.store.lock().await.get_current_slot().unwrap() + 10;
        incoming_beacon_block.message.slot = future_slot;

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
        assert!(
            matches!(result, DependencyValidationResult::Ignore(reason) if reason.contains("future slot"))
        );
    }

    #[tokio::test]
    pub async fn test_block_at_or_before_finalized_slot_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let mut ancestor_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/ancestor_9551968.ssz_snappy",
        )
        .unwrap();
        ancestor_block.message.slot = 0;

        let result = validate_gossip_beacon_block(&beacon_chain, &cached_db, &ancestor_block)
            .await
            .unwrap();
        assert!(
            matches!(result, DependencyValidationResult::Ignore(reason) if reason.contains("latest finalized slot"))
        );
    }

    #[tokio::test]
    pub async fn test_validator_not_found_rejects() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let mut incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();

        // Mutate proposer index to a very high index
        incoming_beacon_block.message.proposer_index = 999999;

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
        assert!(
            matches!(result, DependencyValidationResult::Reject(reason) if reason.contains("Validator not found"))
        );
    }

    #[tokio::test]
    pub async fn test_duplicate_proposer_signature_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();

        // Inserting the proposer signature into cache ahead of time
        {
            let state = beacon_chain
                .store
                .lock()
                .await
                .db
                .get_latest_state()
                .unwrap();
            let validator =
                &state.validators[incoming_beacon_block.message.proposer_index as usize];
            cached_db.seen_proposer_signature.write().await.put(
                AddressSlotIdentifier {
                    address: validator.public_key.clone(),
                    slot: incoming_beacon_block.message.slot,
                },
                incoming_beacon_block.signature.clone(),
            );
        }

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
        assert!(
            matches!(result, DependencyValidationResult::Ignore(reason) if reason.contains("already received"))
        );
    }

    #[tokio::test]
    pub async fn test_unrelated_bls_change_cache_entry_does_not_ignore_block() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/child_9552076.ssz_snappy",
        )
        .unwrap();
        assert!(
            incoming_beacon_block
                .message
                .body
                .bls_to_execution_changes
                .is_empty()
        );

        {
            let state = beacon_chain
                .store
                .lock()
                .await
                .db
                .get_latest_state()
                .unwrap();
            let validator =
                &state.validators[incoming_beacon_block.message.proposer_index as usize];
            cached_db.seen_bls_to_execution_signature.write().await.put(
                AddressSlotIdentifier {
                    address: validator.public_key.clone(),
                    slot: incoming_beacon_block.message.slot,
                },
                BLSToExecutionChange {
                    validator_index: 0,
                    from_bls_public_key: Default::default(),
                    to_execution_address: Default::default(),
                },
            );
        }

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
        assert_eq!(result, DependencyValidationResult::Accept);
    }

    fn read_ssz_snappy_file<T: Decode>(path: &str) -> anyhow::Result<T> {
        let path = PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(path);

        let ssz_snappy = std::fs::read(path)?;
        let mut decoder = Decoder::new();
        let ssz = decoder.decompress_vec(&ssz_snappy)?;
        T::from_ssz_bytes(&ssz).map_err(|err| anyhow!("Failed to decode SSZ: {err:?}"))
    }
}
