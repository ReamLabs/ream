#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use alloy_primitives::B256;
    use ream_consensus_lean::{
        attestation::{Attestation, AttestationData, SignedAttestation},
        block::{
            Block, BlockBody, BlockHeader, BlockWithAttestation, BlockWithSignatures,
            SignedBlockWithAttestation,
        },
        checkpoint::Checkpoint,
        config::Config,
        state::LeanState,
        validator::Validator,
    };
    use ream_fork_choice_lean::store::Store;
    use ream_network_spec::networks::{LeanNetworkSpec, set_lean_network_spec};
    use ream_network_state_lean::NetworkState;
    use ream_post_quantum_crypto::hashsig::signature::Signature;
    use ream_storage::{
        db::{ReamDB, lean::LeanDB},
        tables::{field::REDBField, table::REDBTable},
    };
    use ssz_types::VariableList;
    use tempdir::TempDir;
    use tree_hash::TreeHash;

    pub fn get_config() -> Config {
        Config { genesis_time: 0 }
    }

    pub fn db_setup() -> LeanDB {
        let temp_dir = TempDir::new("lean_test").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let ream_db = ReamDB::new(temp_path).expect("unable to init Ream Database");
        ream_db.init_lean_db().unwrap()
    }

    pub fn sample_state() -> LeanState {
        let config = get_config();
        LeanState::generate_genesis(
            config.genesis_time,
            Some(Validator::generate_default_validators(10)),
        )
    }

    pub async fn sample_store(genesis_state: &mut LeanState) -> Store {
        let genesis_block = Block {
            slot: 0,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root: genesis_state.tree_hash_root(),
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };
        let genesis_block_hash = genesis_block.tree_hash_root();

        let genesis_header = BlockHeader {
            slot: genesis_block.slot,
            proposer_index: genesis_block.proposer_index,
            parent_root: genesis_block.parent_root,
            state_root: genesis_block.state_root,
            body_root: genesis_block.body.tree_hash_root(),
        };

        let signed_genesis_block = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: genesis_block,
                proposer_attestation: Attestation {
                    validator_id: 0,
                    data: AttestationData {
                        slot: 0,
                        head: Checkpoint::default(),
                        target: Checkpoint::default(),
                        source: Checkpoint::default(),
                    },
                },
            },
            signature: VariableList::default(),
        };
        let checkpoint = Checkpoint {
            slot: 0,
            root: genesis_block_hash,
        };

        set_lean_network_spec(LeanNetworkSpec::ephemery().into());
        genesis_state.latest_block_header = genesis_header;
        genesis_state.latest_finalized = checkpoint;
        genesis_state.latest_justified = checkpoint;

        let db = db_setup();

        db.time_provider().insert(100).unwrap();
        db.block_provider()
            .insert(genesis_block_hash, signed_genesis_block.clone())
            .unwrap();
        db.latest_finalized_provider().insert(checkpoint).unwrap();
        db.latest_justified_provider().insert(checkpoint).unwrap();
        db.head_provider().insert(genesis_block_hash).unwrap();
        db.safe_target_provider()
            .insert(genesis_block_hash)
            .unwrap();
        db.state_provider()
            .insert(genesis_block_hash, genesis_state.clone())
            .unwrap();

        Store {
            store: Arc::new(tokio::sync::Mutex::new(db)),
            network_state: Arc::new(NetworkState::new(checkpoint, checkpoint)),
        }
    }

    pub fn build_signed_attestation(
        validator: u64,
        slot: u64,
        head: Option<Checkpoint>,
        source: Option<Checkpoint>,
        target: Option<Checkpoint>,
    ) -> SignedAttestation {
        let data = Attestation {
            validator_id: validator,
            data: AttestationData {
                slot,
                head: head.unwrap_or_default(),
                target: target.unwrap_or_default(),
                source: source.unwrap_or_default(),
            },
        };
        SignedAttestation {
            message: data,
            signature: Signature::blank(),
        }
    }

    pub fn _build_signed_block_with_attestation(
        signed_attestation: SignedAttestation,
        block: BlockWithSignatures,
    ) -> SignedBlockWithAttestation {
        SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: block.block,
                proposer_attestation: signed_attestation.message,
            },
            signature: block.signatures,
        }
    }

    #[tokio::test]
    async fn test_produce_block_basic() {
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut store = sample_store(&mut genesis_state).await;

        genesis_state.process_slots(1).unwrap();
        let store_head = store.store.lock().await.head_provider().get().unwrap();

        let BlockWithSignatures {
            block,
            mut signatures,
        } = store.produce_block_with_signatures(1, 1).await.unwrap();

        assert_eq!(block.slot, 1);
        assert_eq!(block.proposer_index, 1);
        assert_eq!(block.parent_root, store_head);
        assert_ne!(block.state_root, B256::ZERO);

        let attestation_data = store.produce_attestation(1).await.unwrap();
        let message = Attestation {
            validator_id: 1,
            data: attestation_data,
        };
        signatures.push(Signature::blank()).unwrap();
        let signed_block_with_attestation = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: block.clone(),
                proposer_attestation: message,
            },
            signature: signatures,
        };

        store
            .on_block(&signed_block_with_attestation)
            .await
            .unwrap();

        assert!(
            store
                .store
                .lock()
                .await
                .block_provider()
                .get(block.tree_hash_root())
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .store
                .lock()
                .await
                .state_provider()
                .get(block.tree_hash_root())
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_produce_block_unauthorized_proposer() {
        let store = sample_store(&mut sample_state()).await;

        let block_with_signature = store.produce_block_with_signatures(1, 2).await;
        assert!(block_with_signature.is_err());
    }

    #[tokio::test]
    async fn test_produce_block_with_attestations() {
        let store = sample_store(&mut sample_state()).await;

        let (head_provider, block_provider, justified_provider, latest_known_attestations) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.latest_justified_provider(),
                db.latest_known_attestations_provider(),
            )
        };
        let head = head_provider.get().unwrap();
        let head_block = block_provider.get(head).unwrap().unwrap();
        let justified_checkpoint = justified_provider.get().unwrap();
        let attestation_target = store.get_attestation_target().await.unwrap();

        let attestation1 = build_signed_attestation(
            5,
            head_block.message.block.slot,
            Some(Checkpoint {
                root: head,
                slot: head_block.message.block.slot,
            }),
            Some(justified_checkpoint),
            Some(attestation_target),
        );

        let attestation2 = build_signed_attestation(
            6,
            head_block.message.block.slot,
            Some(Checkpoint {
                root: head,
                slot: head_block.message.block.slot,
            }),
            Some(justified_checkpoint),
            Some(attestation_target),
        );
        latest_known_attestations
            .batch_insert([(5, attestation1), (6, attestation2)])
            .unwrap();

        let block_with_signature = store.produce_block_with_signatures(2, 2).await.unwrap();

        assert!(!block_with_signature.block.body.attestations.is_empty());
        assert_eq!(block_with_signature.block.slot, 2);
        assert_eq!(block_with_signature.block.proposer_index, 2);
        assert_eq!(
            block_with_signature.block.parent_root,
            store.get_proposal_head(2).await.unwrap()
        );
        assert_ne!(block_with_signature.block.state_root, B256::ZERO);
    }

    #[tokio::test]
    pub async fn test_produce_block_sequential_slots() {
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let store = sample_store(&mut genesis_state).await;

        genesis_state.process_slots(1).unwrap();
        let genesis_hash = store.store.lock().await.head_provider().get().unwrap();
        // println!(
        //     "head for slot 1: {:?}",
        //     store.get_proposal_head(1).await.unwrap()
        // );

        let BlockWithSignatures {
            block,
            mut signatures,
        } = store.produce_block_with_signatures(1, 1).await.unwrap();
        assert_eq!(block.parent_root, genesis_hash);

        let attestation_data = store.produce_attestation(1).await.unwrap();
        let message = Attestation {
            validator_id: 1,
            data: attestation_data,
        };
        signatures.push(Signature::blank()).unwrap();
        let signed_block_with_attestation = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: block.clone(),
                proposer_attestation: message,
            },
            signature: signatures,
        };

        store
            .store
            .lock()
            .await
            .block_provider()
            .insert(block.tree_hash_root(), signed_block_with_attestation)
            .unwrap();

        // println!(
        //     "head for slot 2: {:?}",
        //     store.get_proposal_head(2).await.unwrap()
        // );

        let BlockWithSignatures {
            block,
            signatures: _,
        } = store.produce_block_with_signatures(2, 2).await.unwrap();

        assert_eq!(block.parent_root, genesis_hash);
    }

    #[tokio::test]
    pub async fn test_produce_block_empty_attestations() {
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let store = sample_store(&mut genesis_state).await;
        let head = store.get_proposal_head(3).await.unwrap();

        let BlockWithSignatures {
            block,
            signatures: _,
        } = store.produce_block_with_signatures(3, 3).await.unwrap();

        assert_eq!(block.body.attestations.len(), 0);
        assert_eq!(block.slot, 3);
        assert_eq!(block.parent_root, head);
        assert!(!block.state_root.is_zero());
    }

    #[tokio::test]
    pub async fn test_produce_block_state_consistency() {
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut store = sample_store(&mut genesis_state).await;
        let head = store.get_proposal_head(3).await.unwrap();
        let (block_provider, state_provider, latest_known_attestations, latest_justified_provider) = {
            let store = store.store.lock().await;
            (
                store.block_provider(),
                store.state_provider(),
                store.latest_known_attestations_provider(),
                store.latest_justified_provider(),
            )
        };
        let head_block = block_provider.get(head).unwrap().unwrap();

        let attestation = build_signed_attestation(
            7,
            head_block.message.block.slot,
            Some(Checkpoint {
                root: head,
                slot: head_block.message.block.slot,
            }),
            latest_justified_provider.get().ok(),
            store.get_attestation_target().await.ok(),
        );
        latest_known_attestations.insert(7, attestation).unwrap();

        let BlockWithSignatures {
            block,
            mut signatures,
        } = store.produce_block_with_signatures(4, 4).await.unwrap();

        let block_hash = block.tree_hash_root();

        let attestation_data = store.produce_attestation(4).await.unwrap();
        let message = Attestation {
            validator_id: 4,
            data: attestation_data,
        };
        signatures.push(Signature::blank()).unwrap();
        let signed_block_with_attestation = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: block.clone(),
                proposer_attestation: message,
            },
            signature: signatures,
        };

        store
            .on_block(&signed_block_with_attestation)
            .await
            .unwrap();

        let latest_state = state_provider.get(block_hash).unwrap().unwrap();
        assert_eq!(block.state_root, latest_state.tree_hash_root());
    }
}
