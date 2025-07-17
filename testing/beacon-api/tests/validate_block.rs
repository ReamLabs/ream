mod tests {
    const PATH_TO_TEST_DATA_FOLDER: &str = "./tests";
    use std::{
        fs,
        path::{Path, PathBuf},
        str::FromStr,
    };

    use alloy_primitives::B256;
    use ream_beacon_api_types::responses::BeaconVersionedResponse;
    use ream_beacon_chain::beacon_chain::BeaconChain;
    use ream_consensus::{
        checkpoint::Checkpoint,
        electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    };
    use ream_network_manager::gossipsub::validate::{
        beacon_block::validate_gossip_beacon_block, result::ValidationResult,
    };
    use ream_operation_pool::OperationPool;
    use ream_storage::{
        cache::CachedDB,
        db::ReamDB,
        tables::{Field, Table},
    };
    use serde_json::Value;

    const SEPOLIA_GENESIS_TIME: u64 = 1655733600;
    const CURRENT_TIME: u64 = 1752744600;

    pub async fn db_setup() -> (BeaconChain, CachedDB, B256) {
        let temp = std::path::PathBuf::from("ream_gossip_test");
        fs::create_dir_all(&temp).unwrap();
        let mut db = ReamDB::new(temp).unwrap();

        let ancestor_json_block =
            read_json_file("./assets/sepolia/blocks/slot_8084160.json").unwrap();
        let ancestor_beacon_block: BeaconVersionedResponse<SignedBeaconBlock> =
            serde_json::from_value(ancestor_json_block.clone()).unwrap();

        let grandparent_json_state =
            read_json_file("./assets/sepolia/states/slot_8084248.json").unwrap();
        let grandparent_beacon_state: BeaconVersionedResponse<BeaconState> =
            serde_json::from_value(grandparent_json_state.clone()).unwrap();

        let grandparent_json_block =
            read_json_file("./assets/sepolia/blocks/slot_8084248.json").unwrap();
        let grandparent_beacon_block: BeaconVersionedResponse<SignedBeaconBlock> =
            serde_json::from_value(grandparent_json_block.clone()).unwrap();

        let parent_json_state =
            read_json_file("./assets/sepolia/states/slot_8084249.json").unwrap();
        let parent_beacon_state: BeaconVersionedResponse<BeaconState> =
            serde_json::from_value(parent_json_state.clone()).unwrap();

        let parent_json_block =
            read_json_file("./assets/sepolia/blocks/slot_8084249.json").unwrap();
        let parent_beacon_block: BeaconVersionedResponse<SignedBeaconBlock> =
            serde_json::from_value(parent_json_block.clone()).unwrap();

        let block_root = parent_beacon_block.data.message.block_root();
        let grandparent_block_root = grandparent_beacon_block.data.message.block_root();
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
        ancestor_beacon_block: BeaconVersionedResponse<SignedBeaconBlock>,
        grandparent_block_root: B256,
        block_root: B256,
        grandparent_beacon_state: BeaconVersionedResponse<BeaconState>,
        grandparent_beacon_block: BeaconVersionedResponse<SignedBeaconBlock>,
        parent_beacon_block: BeaconVersionedResponse<SignedBeaconBlock>,
        parent_beacon_state: BeaconVersionedResponse<BeaconState>,
    ) {
        let ancestor_checkpoint = Checkpoint {
            epoch: ancestor_beacon_block.data.message.slot / 32,
            root: ancestor_beacon_block.data.message.block_root(),
        };
        db.beacon_block_provider()
            .insert(
                ancestor_beacon_block.data.message.block_root(),
                ancestor_beacon_block.data,
            )
            .unwrap();

        let slot = parent_beacon_block.data.message.slot;
        db.finalized_checkpoint_provider()
            .insert(ancestor_checkpoint)
            .unwrap();
        db.beacon_block_provider()
            .insert(grandparent_block_root, grandparent_beacon_block.data)
            .unwrap();
        db.beacon_state_provider()
            .insert(grandparent_block_root, grandparent_beacon_state.data)
            .unwrap();
        db.beacon_block_provider()
            .insert(block_root, parent_beacon_block.data)
            .unwrap();
        db.beacon_state_provider()
            .insert(block_root, parent_beacon_state.data)
            .unwrap();
        db.slot_index_provider().insert(slot, block_root).unwrap();
        db.genesis_time_provider()
            .insert(SEPOLIA_GENESIS_TIME)
            .unwrap();
        db.time_provider().insert(CURRENT_TIME).unwrap();
    }

    #[tokio::test]
    pub async fn test_validate_beacon_block() {
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

        let incoming_json_block =
            read_json_file("./assets/sepolia/blocks/slot_8084250.json").unwrap();
        let incoming_beacon_block: BeaconVersionedResponse<SignedBeaconBlock> =
            serde_json::from_value(incoming_json_block.clone()).unwrap();

        assert_eq!(incoming_beacon_block.data.message.slot, 8084250);
        assert_eq!(
            incoming_beacon_block.data.message.block_root(),
            B256::from_str("0x9ad84061d301d8b2d2613ffcb83a937a35f789b52ec1975005ef3c6c9faa3c43")
                .unwrap()
        );

        let result =
            validate_gossip_beacon_block(&beacon_chain, &cached_db, &incoming_beacon_block.data)
                .await
                .unwrap();

        assert!(result == ValidationResult::Accept);
    }

    pub fn read_json_file<P: AsRef<Path>>(file_name: P) -> anyhow::Result<Value> {
        let path = PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(file_name);
        let file_contents = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&file_contents)?)
    }
}
