mod tests {
    const PATH_TO_TEST_DATA_FOLDER: &str = "./tests";
    use std::{fs, path::PathBuf, str::FromStr};

    use alloy_primitives::B256;
    use anyhow::anyhow;
    use ream_beacon_chain::beacon_chain::BeaconChain;
    use ream_consensus_beacon::electra::{
        beacon_block::SignedBeaconBlock, beacon_state::BeaconState,
    };
    use ream_consensus_misc::checkpoint::Checkpoint;
    use ream_network_manager::gossipsub::validate::{
        beacon_block::validate_gossip_beacon_block, result::ValidationResult,
    };
    use ream_network_spec::networks::initialize_test_network_spec;
    use ream_operation_pool::OperationPool;
    use ream_storage::{
        cache::CachedDB,
        db::ReamDB,
        tables::{Field, Table},
    };
    use snap::raw::Decoder;
    use ssz::Decode;

    const SEPOLIA_GENESIS_TIME: u64 = 1655733600;
    const CURRENT_TIME: u64 = 1752744600;

    pub async fn db_setup() -> (BeaconChain, CachedDB, B256) {
        let temp = std::path::PathBuf::from("ream_gossip_test");
        fs::create_dir_all(&temp).unwrap();
        let mut db = ReamDB::new(temp).unwrap();

        let ancestor_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084160.ssz_snappy",
        )
        .unwrap();

        let grandparent_beacon_state =
            read_ssz_snappy_file::<BeaconState>("./assets/sepolia/states/slot_8084248.ssz_snappy")
                .unwrap();

        let grandparent_beacon_block = read_ssz_snappy_file::<SignedBeaconBlock>(
            "./assets/sepolia/blocks/slot_8084248.ssz_snappy",
        )
        .unwrap();

        let parent_beacon_state =
            read_ssz_snappy_file::<BeaconState>("./assets/sepolia/states/slot_8084249.ssz_snappy")
                .unwrap();

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
        let cached_db = CachedDB::default();
        let beacon_chain = BeaconChain::new(db, operation_pool.into(), None);

        (beacon_chain, cached_db, block_root)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_mock_data(
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

    #[tokio::test]
    pub async fn test_validate_beacon_block() {
        initialize_test_network_spec();
        let (beacon_chain, cached_db, block_root) = db_setup().await;

        let (latest_state_in_db, latest_block) = {
            let store = beacon_chain.store.lock().await;

            (
                store.db.get_latest_state().unwrap(),
                store
                    .db
                    .beacon_block_provider()
                    .get(block_root)
                    .unwrap()
                    .unwrap(),
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

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block)
                .await
                .unwrap();

        assert!(result == ValidationResult::Accept);
    }

    fn read_ssz_snappy_file<T: Decode>(path: &str) -> anyhow::Result<T> {
        let path = PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(path);

        let ssz_snappy = std::fs::read(path)?;
        let mut decoder = Decoder::new();
        let ssz = decoder.decompress_vec(&ssz_snappy)?;
        T::from_ssz_bytes(&ssz).map_err(|err| anyhow!("Failed to decode SSZ: {err:?}"))
    }
}
