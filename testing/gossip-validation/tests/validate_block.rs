#[allow(clippy::unwrap_used)]
mod tests {
    const PATH_TO_TEST_DATA_FOLDER: &str = "./tests";
    use std::{path::PathBuf, str::FromStr, sync::Arc};

    use alloy_primitives::B256;
    use anyhow::anyhow;
    use ream_chain_beacon::beacon_chain::BeaconChain;
    use ream_consensus_beacon::{
        bls_to_execution_change::BLSToExecutionChange,
        electra::{
            beacon_block::SignedBeaconBlock, beacon_state::BeaconState,
            zkvm_types::ValidatorRegistryLimit,
        },
        historical_summary::HistoricalSummary,
        pending_consolidation::PendingConsolidation,
        pending_deposit::PendingDeposit,
        pending_partial_withdrawal::PendingPartialWithdrawal,
        sync_committee::SyncCommittee,
    };
    use ream_consensus_misc::{
        beacon_block_header::BeaconBlockHeader, checkpoint::Checkpoint, eth_1_data::Eth1Data,
        validator::Validator,
    };
    use ream_execution_rpc_types::electra::execution_payload_header::ExecutionPayloadHeader;
    use ream_network_manager::gossipsub::validate::{
        beacon_block::validate_gossip_beacon_block, result::ValidationResult,
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
    use ssz_derive::{Decode, Encode};
    use ssz_types::{
        BitVector, FixedVector, VariableList,
        typenum::{U4, U2048, U8192, U65536, U16777216},
    };
    use tempdir::TempDir;

    const SEPOLIA_GENESIS_TIME: u64 = 1655733600;
    const CURRENT_TIME: u64 = 1752744600;

    pub async fn db_setup() -> (BeaconChain, Arc<BeaconCacheDB>, B256) {
        let temp_dir = TempDir::new("ream_gossip_test").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let ream_db = ReamDB::new(temp_path).expect("unable to init Ream Database");
        let cached_db = Arc::new(BeaconCacheDB::default());
        let mut db = ream_db
            .init_beacon_db()
            .unwrap()
            .with_cache(cached_db.clone());

        let ancestor_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084160.ssz_snappy",
        )
        .unwrap();

        let grandparent_beacon_state: BeaconState = read_ssz_snappy_file::<LegacyBeaconState>(
            "./assets/sepolia/states/slot_8084248.ssz_snappy",
        )
        .unwrap()
        .into();

        let grandparent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084248.ssz_snappy",
        )
        .unwrap();

        let parent_beacon_state: BeaconState = read_ssz_snappy_file::<LegacyBeaconState>(
            "./assets/sepolia/states/slot_8084249.ssz_snappy",
        )
        .unwrap()
        .into();

        let parent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084249.ssz_snappy",
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
            parent_beacon_block,
            parent_beacon_state,
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

        (beacon_chain, cached_db, block_root)
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
    ) {
        let ancestor_checkpoint = Checkpoint {
            epoch: ancestor_block.message.slot / 32,
            root: ancestor_block.message.block_root(),
        };
        db.block_provider()
            .insert(ancestor_block.message.block_root(), ancestor_block)
            .unwrap();

        let slot = parent_block.message.slot;
        db.finalized_checkpoint_provider()
            .insert(ancestor_checkpoint)
            .unwrap();
        db.block_provider()
            .insert(grandparent_block_root, grandparent_block)
            .unwrap();
        db.state_provider()
            .insert(grandparent_block_root, grandparent_state)
            .unwrap();
        db.block_provider()
            .insert(block_root, parent_block)
            .unwrap();
        db.state_provider()
            .insert(block_root, parent_state)
            .unwrap();
        db.slot_index_provider().insert(slot, block_root).unwrap();
        db.genesis_time_provider()
            .insert(SEPOLIA_GENESIS_TIME)
            .unwrap();
        db.time_provider().insert(CURRENT_TIME).unwrap();
    }

    /// TODO: Fix test for Fulu (Proposer index mismatch)
    #[ignore]
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
        assert_eq!(latest_block.message.slot, 8084249);

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084250.ssz_snappy",
        )
        .unwrap();

        assert_eq!(incoming_beacon_block.message.slot, 8084250);
        assert_eq!(
            incoming_beacon_block.message.block_root(),
            B256::from_str("0x9ad84061d301d8b2d2613ffcb83a937a35f789b52ec1975005ef3c6c9faa3c43")
                .unwrap()
        );

        let _result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
    }

    #[tokio::test]
    pub async fn test_future_slot_block_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let mut incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084250.ssz_snappy",
        )
        .unwrap();
        let future_slot = beacon_chain.store.lock().await.get_current_slot().unwrap() + 10;
        incoming_beacon_block.message.slot = future_slot;

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
        assert!(
            matches!(result, ValidationResult::Ignore(reason) if reason.contains("future slot"))
        );
    }

    #[tokio::test]
    pub async fn test_block_at_or_before_finalized_slot_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let ancestor_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084160.ssz_snappy",
        )
        .unwrap();

        let result = validate_gossip_beacon_block(&beacon_chain, &cached_db, &ancestor_block)
            .await
            .unwrap();
        assert!(
            matches!(result, ValidationResult::Ignore(reason) if reason.contains("latest finalized slot"))
        );
    }

    #[tokio::test]
    pub async fn test_validator_not_found_rejects() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let mut incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084250.ssz_snappy",
        )
        .unwrap();

        // Mutate proposer index to a very high index
        incoming_beacon_block.message.proposer_index = 999999;

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();
        assert!(
            matches!(result, ValidationResult::Reject(reason) if reason.contains("Validator not found"))
        );
    }

    #[tokio::test]
    pub async fn test_duplicate_proposer_signature_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084250.ssz_snappy",
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
            matches!(result, ValidationResult::Ignore(reason) if reason.contains("already received"))
        );
    }

    /// TODO: Fix test for Fulu (Proposer index mismatch)
    #[ignore]
    #[tokio::test]
    pub async fn test_bls_to_execution_change_duplicate_is_ignored() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, _block_root) = db_setup().await;

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084250.ssz_snappy",
        )
        .unwrap();

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
        assert!(
            matches!(result, ValidationResult::Ignore(reason) if reason.contains("Signature already received"))
        );
    }

    fn read_ssz_snappy_file<T: Decode>(path: &str) -> anyhow::Result<T> {
        let path = PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(path);

        let ssz_snappy = std::fs::read(path)?;
        let mut decoder = Decoder::new();
        let ssz = decoder.decompress_vec(&ssz_snappy)?;
        T::from_ssz_bytes(&ssz).map_err(|err| anyhow!("Failed to decode SSZ: {err:?}"))
    }

    #[derive(Debug, PartialEq, Clone, Encode, Decode)]
    pub struct LegacyBeaconState {
        pub genesis_time: u64,
        pub genesis_validators_root: B256,
        pub slot: u64,
        pub fork: ream_consensus_misc::fork::Fork,
        pub latest_block_header: BeaconBlockHeader,
        pub block_roots: FixedVector<B256, U8192>,
        pub state_roots: FixedVector<B256, U8192>,
        pub historical_roots: VariableList<B256, U16777216>,
        pub eth1_data: Eth1Data,
        pub eth1_data_votes: VariableList<Eth1Data, U2048>,
        pub eth1_deposit_index: u64,
        pub validators: VariableList<Validator, ValidatorRegistryLimit>,
        pub balances: VariableList<u64, ValidatorRegistryLimit>,
        pub randao_mixes: FixedVector<B256, U65536>,
        pub slashings: FixedVector<u64, U8192>,
        pub previous_epoch_participation: VariableList<u8, ValidatorRegistryLimit>,
        pub current_epoch_participation: VariableList<u8, ValidatorRegistryLimit>,
        pub justification_bits: BitVector<U4>,
        pub previous_justified_checkpoint: Checkpoint,
        pub current_justified_checkpoint: Checkpoint,
        pub finalized_checkpoint: Checkpoint,
        pub inactivity_scores: VariableList<u64, ValidatorRegistryLimit>,
        pub current_sync_committee: Arc<SyncCommittee>,
        pub next_sync_committee: Arc<SyncCommittee>,
        pub latest_execution_payload_header: ExecutionPayloadHeader,
        pub next_withdrawal_index: u64,
        pub next_withdrawal_validator_index: u64,
        pub historical_summaries: VariableList<HistoricalSummary, U16777216>,
        pub deposit_requests_start_index: u64,
        pub deposit_balance_to_consume: u64,
        pub exit_balance_to_consume: u64,
        pub earliest_exit_epoch: u64,
        pub consolidation_balance_to_consume: u64,
        pub earliest_consolidation_epoch: u64,
        pub pending_deposits: VariableList<PendingDeposit, ssz_types::typenum::U134217728>,
        pub pending_partial_withdrawals:
            VariableList<PendingPartialWithdrawal, ssz_types::typenum::U134217728>,
        pub pending_consolidations: VariableList<PendingConsolidation, ssz_types::typenum::U262144>,
    }

    impl From<LegacyBeaconState> for BeaconState {
        fn from(state: LegacyBeaconState) -> Self {
            BeaconState {
                genesis_time: state.genesis_time,
                genesis_validators_root: state.genesis_validators_root,
                slot: state.slot,
                fork: state.fork,
                latest_block_header: state.latest_block_header,
                block_roots: state.block_roots,
                state_roots: state.state_roots,
                historical_roots: state.historical_roots,
                eth1_data: state.eth1_data,
                eth1_data_votes: state.eth1_data_votes,
                eth1_deposit_index: state.eth1_deposit_index,
                validators: state.validators,
                balances: state.balances,
                randao_mixes: state.randao_mixes,
                slashings: state.slashings,
                previous_epoch_participation: state.previous_epoch_participation,
                current_epoch_participation: state.current_epoch_participation,
                justification_bits: state.justification_bits,
                previous_justified_checkpoint: state.previous_justified_checkpoint,
                current_justified_checkpoint: state.current_justified_checkpoint,
                finalized_checkpoint: state.finalized_checkpoint,
                inactivity_scores: state.inactivity_scores,
                current_sync_committee: state.current_sync_committee,
                next_sync_committee: state.next_sync_committee,
                latest_execution_payload_header: state.latest_execution_payload_header,
                next_withdrawal_index: state.next_withdrawal_index,
                next_withdrawal_validator_index: state.next_withdrawal_validator_index,
                historical_summaries: state.historical_summaries,
                deposit_requests_start_index: state.deposit_requests_start_index,
                deposit_balance_to_consume: state.deposit_balance_to_consume,
                exit_balance_to_consume: state.exit_balance_to_consume,
                earliest_exit_epoch: state.earliest_exit_epoch,
                consolidation_balance_to_consume: state.consolidation_balance_to_consume,
                earliest_consolidation_epoch: state.earliest_consolidation_epoch,
                pending_deposits: state.pending_deposits,
                pending_partial_withdrawals: state.pending_partial_withdrawals,
                pending_consolidations: state.pending_consolidations,
                // Fulu fields
                proposer_lookahead: Default::default(),
            }
        }
    }
}
