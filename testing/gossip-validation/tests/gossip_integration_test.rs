mod tests {
    use std::{path::PathBuf, str::FromStr, sync::Arc, time::Duration};

    use alloy_primitives::B256;
    use anyhow::anyhow;
    use libp2p::gossipsub::Message;
    use ream_consensus_beacon::electra::{
        beacon_block::SignedBeaconBlock, beacon_state::BeaconState,
    };
    use ream_consensus_misc::checkpoint::Checkpoint;
    use ream_executor::ReamExecutor;
    use ream_network_manager::{config::ManagerConfig, service::NetworkManagerService};
    use ream_network_spec::networks::initialize_test_network_spec;
    use ream_operation_pool::OperationPool;
    use ream_p2p::{bootnodes::Bootnodes, gossipsub::beacon::topics::GossipTopic};
    use ream_storage::{
        cache::CachedDB,
        db::ReamDB,
        tables::{Field, Table},
    };
    use snap::raw::Decoder;
    use ssz::{Decode, Encode};
    use tempdir::TempDir;
    use tracing::info;

    const PATH_TO_TEST_DATA_FOLDER: &str = "./tests";
    const SEPOLIA_GENESIS_TIME: u64 = 1655733600;
    const CURRENT_TIME: u64 = 1752744600;

    /// Sets up a complete network manager service with test data
    async fn setup_network_manager_service()
    -> anyhow::Result<(NetworkManagerService, CachedDB, B256, SignedBeaconBlock)> {
        let temp_dir = TempDir::new("ream_gossip_integration_test").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let mut db = ReamDB::new(temp_path.clone()).unwrap();

        let ancestor_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084160.ssz_snappy",
        )
        .map_err(|e| anyhow!("Failed to load ancestor block: {}", e))?;

        let grandparent_beacon_state =
            read_ssz_snappy_file::<BeaconState>("./assets/sepolia/states/slot_8084248.ssz_snappy")
                .map_err(|e| anyhow!("Failed to load grandparent state: {}", e))?;

        let grandparent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084248.ssz_snappy",
        )
        .map_err(|e| anyhow!("Failed to load grandparent block: {}", e))?;

        let parent_beacon_state =
            read_ssz_snappy_file::<BeaconState>("./assets/sepolia/states/slot_8084249.ssz_snappy")
                .map_err(|e| anyhow!("Failed to load parent state: {}", e))?;

        let parent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084249.ssz_snappy",
        )
        .map_err(|e| anyhow!("Failed to load parent block: {}", e))?;

        let incoming_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084250.ssz_snappy",
        )
        .map_err(|e| anyhow!("Failed to load incoming block: {}", e))?;

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

        let config = ManagerConfig {
            http_address: "127.0.0.1".parse().unwrap(),
            http_port: 5052,
            http_allow_origin: false,
            socket_address: "127.0.0.1".parse().unwrap(),
            socket_port: 9000,
            discovery_port: 9001,
            disable_discovery: true, // Disable discovery for testing
            data_dir: None,
            ephemeral: true,
            bootnodes: Bootnodes::default(),
            checkpoint_sync_url: None,
            purge_db: false,
            execution_endpoint: None,
            execution_jwt_secret: None,
        };

        let executor = ReamExecutor::new().unwrap();
        let operation_pool = Arc::new(OperationPool::default());

        // Create network manager service
        let network_manager =
            NetworkManagerService::new(executor, config, db, temp_path.clone(), operation_pool)
                .await?;

        let cached_db = CachedDB::new();

        Ok((
            network_manager,
            cached_db,
            block_root,
            incoming_beacon_block,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_mock_data(
        db: &mut ReamDB,
        ancestor_beacon_block: SignedBeaconBlock,
        grandparent_block_root: B256,
        block_root: B256,
        grandparent_beacon_state: BeaconState,
        grandparent_beacon_block: SignedBeaconBlock,
        parent_beacon_block: SignedBeaconBlock,
        parent_beacon_state: BeaconState,
    ) {
        let ancestor_checkpoint = Checkpoint {
            epoch: ancestor_beacon_block.message.slot / 32,
            root: ancestor_beacon_block.message.block_root(),
        };
        db.beacon_block_provider()
            .insert(
                ancestor_beacon_block.message.block_root(),
                ancestor_beacon_block,
            )
            .unwrap();

        let slot = parent_beacon_block.message.slot;
        db.finalized_checkpoint_provider()
            .insert(ancestor_checkpoint)
            .unwrap();
        db.beacon_block_provider()
            .insert(grandparent_block_root, grandparent_beacon_block)
            .unwrap();
        db.beacon_state_provider()
            .insert(grandparent_block_root, grandparent_beacon_state)
            .unwrap();
        db.beacon_block_provider()
            .insert(block_root, parent_beacon_block)
            .unwrap();
        db.beacon_state_provider()
            .insert(block_root, parent_beacon_state)
            .unwrap();
        db.slot_index_provider().insert(slot, block_root).unwrap();
        db.genesis_time_provider()
            .insert(SEPOLIA_GENESIS_TIME)
            .unwrap();
        db.time_provider().insert(CURRENT_TIME).unwrap();
    }

    /// Creates a mock gossipsub message from a beacon block
    fn create_gossip_message(block: &SignedBeaconBlock) -> Message {
        use ream_consensus_misc::constants::beacon::genesis_validators_root;
        use ream_network_spec::networks::beacon_network_spec;
        use ream_p2p::gossipsub::beacon::topics::GossipTopicKind;

        let topic = GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BeaconBlock,
        };
        let topic_hash: libp2p::gossipsub::TopicHash = topic.into();
        let data = block.as_ssz_bytes();

        Message {
            source: None,
            data,
            sequence_number: None,
            topic: topic_hash,
        }
    }

    async fn simulate_gossip_reception(
        network_manager: &NetworkManagerService,
        cached_db: &CachedDB,
        block: &SignedBeaconBlock,
    ) -> anyhow::Result<bool> {
        let gossip_message = create_gossip_message(block);

        use ream_network_manager::gossipsub::handle::handle_gossipsub_message;

        handle_gossipsub_message(
            gossip_message,
            &network_manager.beacon_chain,
            cached_db,
            &network_manager.p2p_sender,
        )
        .await;

        tokio::time::sleep(Duration::from_millis(200)).await;

        let block_root = block.message.block_root();
        let block_exists = {
            let store = network_manager.beacon_chain.store.lock().await;
            store
                .db
                .beacon_block_provider()
                .get(block_root)
                .unwrap()
                .is_some()
        };

        Ok(block_exists)
    }

    // test through the gossip hadling
    #[tokio::test]
    pub async fn test_gossip_beacon_block_integration() {
        initialize_test_network_spec();

        info!("Setting up network manager service for gossip integration test");
        let (network_manager, cached_db, _block_root, incoming_block) =
            setup_network_manager_service().await.unwrap();

        info!("Testing gossip reception of valid beacon block");

        assert_eq!(incoming_block.message.slot, 8084250);
        assert_eq!(
            incoming_block.message.block_root(),
            B256::from_str("0x9ad84061d301d8b2d2613ffcb83a937a35f789b52ec1975005ef3c6c9faa3c43")
                .unwrap()
        );

        let result = simulate_gossip_reception(&network_manager, &cached_db, &incoming_block).await;

        assert!(
            result.is_ok(),
            "Gossip reception should complete without errors"
        );

        info!("Gossip integration test completed successfully");
    }

    #[tokio::test]
    pub async fn test_gossip_duplicate_block_handling() {
        initialize_test_network_spec();

        info!("Setting up network manager service for duplicate block test");
        let (network_manager, cached_db, _block_root, incoming_block) =
            setup_network_manager_service().await.unwrap();

        info!("Testing duplicate block handling through gossip");

        let first_result =
            simulate_gossip_reception(&network_manager, &cached_db, &incoming_block).await;
        assert!(
            first_result.is_ok(),
            "First block reception should complete without errors"
        );

        let second_result =
            simulate_gossip_reception(&network_manager, &cached_db, &incoming_block).await;

        assert!(
            second_result.is_ok(),
            "Duplicate block should not cause errors"
        );

        info!("Duplicate block handling test completed successfully");
    }

    #[tokio::test]
    pub async fn test_gossip_invalid_block_rejection() {
        initialize_test_network_spec();

        info!("Setting up network manager service for invalid block test");
        let (network_manager, cached_db, _block_root, mut invalid_block) =
            setup_network_manager_service().await.unwrap();

        invalid_block.message.proposer_index = 999999;

        info!("Testing invalid block rejection through gossip");

        let result = simulate_gossip_reception(&network_manager, &cached_db, &invalid_block).await;

        assert!(result.is_ok(), "Invalid block should be handled gracefully");

        let block_root = invalid_block.message.block_root();
        let block_exists = {
            let store = network_manager.beacon_chain.store.lock().await;
            store
                .db
                .beacon_block_provider()
                .get(block_root)
                .unwrap()
                .is_some()
        };

        assert!(!block_exists, "Invalid block should not be stored");

        info!("Invalid block rejection test completed successfully");
    }

    fn read_ssz_snappy_file<T: Decode>(path: &str) -> anyhow::Result<T> {
        let path = PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(path);

        let ssz_snappy = std::fs::read(path)?;
        let mut decoder = Decoder::new();
        let ssz = decoder.decompress_vec(&ssz_snappy)?;
        T::from_ssz_bytes(&ssz).map_err(|err| anyhow!("Failed to decode SSZ: {err:?}"))
    }
}
