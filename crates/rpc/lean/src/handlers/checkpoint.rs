use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_common::error::ApiError;
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::field::REDBField;

// GET /lean/v0/checkpoints/justified
#[get("/checkpoints/justified")]
pub async fn get_justified_checkpoint(
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    let checkpoint = lean_chain
        .read()
        .await
        .store
        .lock()
        .await
        .latest_justified_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Could not get justified checkpoint: {err:?}"))
        })?;

    Ok(HttpResponse::Ok().json(checkpoint))
}

#[cfg(all(test, feature = "devnet2"))]
mod tests {
    use actix_web::{App, http::StatusCode, test, web::Data};
    use ream_consensus_lean::{
        attestation::{AggregatedAttestations, AttestationData},
        block::{BlockSignatures, BlockWithAttestation, SignedBlockWithAttestation},
        checkpoint::Checkpoint,
        utils::generate_default_validators,
    };
    use ream_fork_choice_lean::{genesis::setup_genesis, store::Store};
    use ream_network_spec::networks::{LeanNetworkSpec, lean_network_spec, set_lean_network_spec};
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use ream_storage::db::ReamDB;
    use ream_sync::rwlock::Writer;
    use ssz_types::VariableList;
    use tree_hash::TreeHash;

    use super::get_justified_checkpoint;

    async fn sample_store(no_of_validators: usize) -> Store {
        set_lean_network_spec(LeanNetworkSpec::ephemery().into());
        let (genesis_block, genesis_state) = setup_genesis(
            lean_network_spec().genesis_time,
            generate_default_validators(no_of_validators),
        );

        let checkpoint = Checkpoint {
            slot: genesis_block.slot,
            root: genesis_block.tree_hash_root(),
        };
        let signed_genesis_block = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                proposer_attestation: AggregatedAttestations {
                    validator_id: genesis_block.proposer_index,
                    data: AttestationData {
                        slot: genesis_block.slot,
                        head: checkpoint,
                        target: checkpoint,
                        source: checkpoint,
                    },
                },
                block: genesis_block,
            },
            signature: BlockSignatures {
                attestation_signatures: VariableList::default(),
                proposer_signature: Signature::blank(),
            },
        };

        let temp_path = std::env::temp_dir().join(format!("lean_test_{}", std::process::id()));
        let ream_db = ReamDB::new(temp_path).expect("Failed to init Ream Database");
        let lean_db = ream_db.init_lean_db().expect("Failed to init lean db");

        Store::get_forkchoice_store(signed_genesis_block, genesis_state, lean_db, Some(0))
            .expect("Failed to create forkchoice store")
    }

    #[tokio::test]
    async fn test_get_justified_checkpoint_returns_json() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app = test::init_service(
            App::new()
                .app_data(Data::new(reader))
                .service(get_justified_checkpoint),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/checkpoints/justified")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("application/json")
        );

        let body = test::read_body(resp).await;
        let checkpoint: Checkpoint = serde_json::from_slice(&body).expect("Failed to decode JSON");
        assert_eq!(checkpoint.slot, 0);
    }
}
