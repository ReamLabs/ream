use anyhow::{Result, anyhow};
use ream_consensus_lean::state::LeanState;
use reqwest::{Client, StatusCode, Url};
use ssz::Decode;

#[derive(Default)]
pub struct LeanCheckpointClient {
    http: Client,
}

impl LeanCheckpointClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub async fn fetch_finalized_state(&self, url: &Url) -> Result<LeanState> {
        let url = url.join("/lean/v0/states/finalized")?;

        let response = self
            .http
            .get(url)
            .header("Accept", "application/octet-stream")
            .send()
            .await?;

        if response.status() != StatusCode::OK {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await?
            ));
        }

        LeanState::from_ssz_bytes(&response.bytes().await?)
            .map_err(|err| anyhow!("SSZ decode failed: {err:?}"))
    }
}

pub fn verify_checkpoint_state(state: &LeanState) -> bool {
    if state.validators.is_empty() {
        return false;
    }

    true
}

#[cfg(all(test, feature = "devnet2"))]
mod tests {
    use std::net::TcpListener;

    use actix_web::{
        App, HttpServer,
        web::{Data, scope},
    };
    use ream_consensus_lean::{
        attestation::{AggregatedAttestations, AttestationData},
        block::{BlockSignatures, BlockWithAttestation, SignedBlockWithAttestation},
        checkpoint::Checkpoint,
        utils::generate_default_validators,
    };
    use ream_fork_choice_lean::{genesis::setup_genesis, store::Store};
    use ream_network_spec::networks::{LeanNetworkSpec, lean_network_spec, set_lean_network_spec};
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use ream_rpc_lean::handlers::state::get_state;
    use ream_storage::db::ReamDB;
    use ream_sync::rwlock::Writer;
    use reqwest::Url;
    use ssz_types::VariableList;
    use tree_hash::TreeHash;

    use super::{LeanCheckpointClient, verify_checkpoint_state};

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

        let temp_path = std::env::temp_dir().join(format!(
            "checkpoint_sync_lean_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&temp_path).expect("Failed to create temp directory");
        let ream_db = ReamDB::new(temp_path).expect("Failed to init Ream Database");
        let lean_db = ream_db.init_lean_db().expect("Failed to init lean db");

        Store::get_forkchoice_store(signed_genesis_block, genesis_state, lean_db, Some(0))
            .expect("Failed to create forkchoice store")
    }

    #[tokio::test]
    async fn test_client_fetches_and_deserializes_state() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
        let addr = listener.local_addr().expect("Failed to get local addr");

        let server = HttpServer::new(move || {
            App::new()
                .app_data(Data::new(reader.clone()))
                .service(scope("/lean/v0").service(get_state))
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        let client = LeanCheckpointClient::new();
        let base_url = Url::parse(&format!("http://{addr}")).expect("Failed to parse base URL");

        let state = client
            .fetch_finalized_state(&base_url)
            .await
            .expect("Client failed to fetch finalized state");

        assert_eq!(state.slot, 0);
        assert!(verify_checkpoint_state(&state));

        let (_, genesis_state) = setup_genesis(
            lean_network_spec().genesis_time,
            generate_default_validators(10),
        );

        assert_eq!(state, genesis_state);

        server_handle.stop(true).await;
    }
}
