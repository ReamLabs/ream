use anyhow::{Result, anyhow, bail};
use ream_consensus_lean::{block::SignedBlock, state::LeanState};
use ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT;
use reqwest::{Client, StatusCode, Url};
use ssz::Decode;
use tracing::warn;
use tree_hash::TreeHash;

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
            bail!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await?
            );
        }

        LeanState::from_ssz_bytes(&response.bytes().await?)
            .map_err(|err| anyhow!("SSZ decode failed: {err:?}"))
    }

    pub async fn fetch_finalized_block(&self, url: &Url) -> Result<SignedBlock> {
        let url = url.join("/lean/v0/blocks/finalized")?;

        let response = self
            .http
            .get(url)
            .header("Accept", "application/octet-stream")
            .send()
            .await?;

        if response.status() != StatusCode::OK {
            bail!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await?
            );
        }

        SignedBlock::from_ssz_bytes(&response.bytes().await?)
            .map_err(|err| anyhow!("SSZ decode failed: {err:?}"))
    }

    pub async fn fetch_finalized_anchor(&self, url: &Url) -> Result<(LeanState, SignedBlock)> {
        let state = self.fetch_finalized_state(url).await?;
        let signed_block = self.fetch_finalized_block(url).await?;

        let expected_state_root = state.tree_hash_root();
        if signed_block.block.state_root != expected_state_root {
            bail!(
                "Anchor block / state mismatch: signed_block.block.state_root={:?} \
                 hash_tree_root(state)={expected_state_root:?}. Server may have advanced \
                 finalization between requests; retry.",
                signed_block.block.state_root,
            );
        }

        Ok((state, signed_block))
    }
}

pub fn verify_checkpoint_state(state: &LeanState) -> Result<()> {
    if state.validators.is_empty() {
        let err = anyhow!("Invalid state: no validators in registry");
        warn!("{err}");
        return Err(err);
    }

    let validator_count = state.validators.len() as u64;
    if state.validators.len() > VALIDATOR_REGISTRY_LIMIT as usize {
        let err = anyhow!(
            "Invalid state: validator count {validator_count} exceeds registry limit {VALIDATOR_REGISTRY_LIMIT}",
        );
        warn!("{err}");
        return Err(err);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        mem::transmute,
        net::TcpListener,
        sync::{Arc, Mutex},
    };

    use actix_web::{
        App, HttpRequest, HttpResponse, HttpServer,
        http::{StatusCode, header},
        web::{self, Data},
    };
    use alloy_primitives::B256;
    #[cfg(feature = "devnet5")]
    use ream_consensus_lean::attestation::MultiMessageAggregate;
    #[cfg(feature = "devnet4")]
    use ream_consensus_lean::block::BlockSignatures;
    use ream_consensus_lean::{
        block::{Block, BlockBody, SignedBlock},
        state::LeanState,
        utils::generate_default_validators,
        validator::Validator,
    };
    use ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT;
    #[cfg(feature = "devnet4")]
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use reqwest::Url;
    use ssz::Encode;
    #[cfg(feature = "devnet5")]
    use ssz_types::typenum::U524288;
    use ssz_types::{
        VariableList,
        typenum::{U4096, U8192},
    };
    use tree_hash::TreeHash;

    use super::{LeanCheckpointClient, verify_checkpoint_state};

    #[derive(Clone)]
    enum ResponseMode {
        State {
            state: LeanState,
            content_type: &'static str,
        },
        Bytes {
            bytes: Vec<u8>,
            content_type: &'static str,
        },
        Error(StatusCode, String),
        CaptureAcceptHeader {
            state: LeanState,
            seen_accept: Arc<Mutex<Option<String>>>,
        },
    }

    async fn finalized_state_response(
        request: HttpRequest,
        mode: Data<ResponseMode>,
    ) -> HttpResponse {
        match mode.get_ref() {
            ResponseMode::State {
                state,
                content_type,
            } => HttpResponse::Ok()
                .content_type(*content_type)
                .body(state.as_ssz_bytes()),
            ResponseMode::Bytes {
                bytes,
                content_type,
            } => HttpResponse::Ok()
                .content_type(*content_type)
                .body(bytes.clone()),
            ResponseMode::Error(status, body) => HttpResponse::build(*status).body(body.clone()),
            ResponseMode::CaptureAcceptHeader { state, seen_accept } => {
                let accept_header = request
                    .headers()
                    .get(header::ACCEPT)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                *seen_accept.lock().expect("Accept mutex poisoned") = accept_header;

                HttpResponse::Ok()
                    .content_type("application/octet-stream")
                    .body(state.as_ssz_bytes())
            }
        }
    }

    fn spawn_checkpoint_server(mode: ResponseMode) -> (Url, actix_web::dev::ServerHandle) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let address = listener.local_addr().expect("Failed to get local address");

        let server = HttpServer::new(move || {
            App::new().app_data(Data::new(mode.clone())).service(
                web::scope("/lean/v0")
                    .route("/states/finalized", web::get().to(finalized_state_response)),
            )
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        (
            Url::parse(&format!("http://{address}")).expect("Failed to parse base URL"),
            server_handle,
        )
    }

    fn spawn_redirect_checkpoint_server(state: LeanState) -> (Url, actix_web::dev::ServerHandle) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let address = listener.local_addr().expect("Failed to get local address");
        let redirect_target = format!("http://{address}/redirected-finalized");

        let server = HttpServer::new(move || {
            let state = state.clone();
            let redirect_target = redirect_target.clone();

            App::new()
                .route(
                    "/lean/v0/states/finalized",
                    web::get().to(move || {
                        let redirect_target = redirect_target.clone();
                        async move {
                            HttpResponse::TemporaryRedirect()
                                .append_header((header::LOCATION, redirect_target))
                                .finish()
                        }
                    }),
                )
                .route(
                    "/redirected-finalized",
                    web::get().to(move || {
                        let state = state.clone();
                        async move {
                            HttpResponse::Ok()
                                .content_type("application/octet-stream")
                                .body(state.as_ssz_bytes())
                        }
                    }),
                )
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        (
            Url::parse(&format!("http://{address}")).expect("Failed to parse base URL"),
            server_handle,
        )
    }

    fn make_state(validator_count: usize) -> LeanState {
        LeanState::generate_genesis(0, Some(generate_default_validators(validator_count)))
    }

    fn make_oversized_state() -> LeanState {
        let mut state =
            make_state(ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT as usize);
        let oversized_validators: VariableList<_, U8192> =
            VariableList::try_from(generate_default_validators(
                (ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT + 1) as usize,
            ))
            .expect("Failed to create oversized validator list");

        // SAFETY: test-only layout cast to exercise the explicit runtime upper-bound check.
        state.validators = unsafe {
            transmute::<VariableList<Validator, U8192>, VariableList<Validator, U4096>>(
                oversized_validators,
            )
        };
        state
    }

    #[tokio::test]
    async fn test_client_fetches_finalized_state_with_and_without_trailing_slash() {
        let expected_state = make_state(10);
        let (base_url, server_handle) = spawn_checkpoint_server(ResponseMode::State {
            state: expected_state.clone(),
            content_type: "application/octet-stream",
        });

        let client = LeanCheckpointClient::new();

        let state = client
            .fetch_finalized_state(&base_url)
            .await
            .expect("Client failed to fetch finalized state");
        assert_eq!(state, expected_state);

        let trailing_slash_url =
            Url::parse(&format!("{base_url}/")).expect("Failed to parse trailing slash URL");
        let state = client
            .fetch_finalized_state(&trailing_slash_url)
            .await
            .expect("Client failed to fetch finalized state with trailing slash URL");
        assert_eq!(state, expected_state);

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_returns_http_error_context() {
        let (base_url, server_handle) = spawn_checkpoint_server(ResponseMode::Error(
            StatusCode::BAD_REQUEST,
            "bad checkpoint request".to_string(),
        ));

        let err = LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect_err("Expected checkpoint fetch to fail");

        let err = err.to_string();
        assert!(err.contains("HTTP error 400"));
        assert!(err.contains("bad checkpoint request"));

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_returns_decode_error_for_malformed_ssz() {
        let (base_url, server_handle) = spawn_checkpoint_server(ResponseMode::Bytes {
            bytes: vec![1, 2, 3, 4],
            content_type: "application/octet-stream",
        });

        let err = LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect_err("Expected malformed SSZ response to fail");

        assert!(err.to_string().contains("SSZ decode failed"));

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_returns_transport_error_when_server_is_unreachable() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let address = listener.local_addr().expect("Failed to get local address");
        drop(listener);

        let base_url = Url::parse(&format!("http://{address}")).expect("Failed to parse base URL");

        LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect_err("Expected checkpoint fetch to fail when server is unreachable");
    }

    #[tokio::test]
    async fn test_client_follows_redirect_to_finalized_state() {
        let expected_state = make_state(10);
        let (base_url, server_handle) = spawn_redirect_checkpoint_server(expected_state.clone());

        let fetched_state = LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect("Expected redirected checkpoint fetch to succeed");

        assert_eq!(fetched_state, expected_state);

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_accepts_valid_ssz_with_non_octet_stream_content_type() {
        let expected_state = make_state(10);
        let (base_url, server_handle) = spawn_checkpoint_server(ResponseMode::State {
            state: expected_state.clone(),
            content_type: "text/plain",
        });

        let state = LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect("Expected valid SSZ bytes to decode regardless of content type");

        assert_eq!(state, expected_state);

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_returns_decode_error_for_empty_body() {
        let (base_url, server_handle) = spawn_checkpoint_server(ResponseMode::Bytes {
            bytes: vec![],
            content_type: "application/octet-stream",
        });

        let err = LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect_err("Expected empty response body to fail SSZ decoding");

        assert!(err.to_string().contains("SSZ decode failed"));

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_sends_octet_stream_accept_header() {
        let seen_accept = Arc::new(Mutex::new(None));
        let (base_url, server_handle) =
            spawn_checkpoint_server(ResponseMode::CaptureAcceptHeader {
                state: make_state(10),
                seen_accept: Arc::clone(&seen_accept),
            });

        LeanCheckpointClient::new()
            .fetch_finalized_state(&base_url)
            .await
            .expect("Expected checkpoint fetch to succeed");

        assert_eq!(
            seen_accept
                .lock()
                .expect("Accept mutex poisoned")
                .clone()
                .as_deref(),
            Some("application/octet-stream")
        );

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_ignores_extra_path_segments_in_base_url() {
        let expected_state = make_state(10);
        let (base_url, server_handle) = spawn_checkpoint_server(ResponseMode::State {
            state: expected_state.clone(),
            content_type: "application/octet-stream",
        });

        let url_with_extra_path = base_url
            .join("/irrelevant/path")
            .expect("Failed to construct base URL with extra path");

        let state = LeanCheckpointClient::new()
            .fetch_finalized_state(&url_with_extra_path)
            .await
            .expect("Expected checkpoint fetch to succeed with extra base path");

        assert_eq!(state, expected_state);

        server_handle.stop(true).await;
    }

    #[test]
    fn test_verify_checkpoint_state_accepts_valid_state() {
        let state = make_state(1);

        verify_checkpoint_state(&state).expect("Expected valid checkpoint state");
    }

    #[test]
    fn test_verify_checkpoint_state_rejects_empty_registry() {
        let state = make_state(0);

        let err =
            verify_checkpoint_state(&state).expect_err("Expected empty validator registry to fail");

        assert_eq!(err.to_string(), "Invalid state: no validators in registry");
    }

    #[test]
    fn test_verify_checkpoint_state_accepts_registry_limit() {
        let state = make_state(VALIDATOR_REGISTRY_LIMIT as usize);

        verify_checkpoint_state(&state).expect("Expected checkpoint state at limit to be valid");
    }

    #[test]
    fn test_verify_checkpoint_state_rejects_registry_above_limit() {
        let state = make_oversized_state();

        let err = verify_checkpoint_state(&state)
            .expect_err("Expected checkpoint state above validator limit to fail");

        assert_eq!(
            err.to_string(),
            format!(
                "Invalid state: validator count {} exceeds registry limit {VALIDATOR_REGISTRY_LIMIT}",
                VALIDATOR_REGISTRY_LIMIT + 1,
            )
        );
    }

    fn make_anchor_signed_block(state: &LeanState) -> SignedBlock {
        let block = Block {
            slot: state.slot,
            proposer_index: state.latest_block_header.proposer_index,
            parent_root: state.latest_block_header.parent_root,
            state_root: state.tree_hash_root(),
            body: BlockBody {
                attestations: Default::default(),
            },
        };
        SignedBlock {
            block,
            #[cfg(feature = "devnet4")]
            signature: BlockSignatures {
                attestation_signatures: VariableList::default(),
                proposer_signature: Signature::blank(),
            },
            #[cfg(feature = "devnet5")]
            proof: MultiMessageAggregate {
                proof: VariableList::<u8, U524288>::default(),
            },
        }
    }

    fn spawn_anchor_server(
        state: LeanState,
        signed_block: SignedBlock,
    ) -> (Url, actix_web::dev::ServerHandle) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let address = listener.local_addr().expect("Failed to get local address");

        let server = HttpServer::new(move || {
            let state = state.clone();
            let signed_block = signed_block.clone();
            App::new().service(
                web::scope("/lean/v0")
                    .route(
                        "/states/finalized",
                        web::get().to(move || {
                            let state = state.clone();
                            async move {
                                HttpResponse::Ok()
                                    .content_type("application/octet-stream")
                                    .body(state.as_ssz_bytes())
                            }
                        }),
                    )
                    .route(
                        "/blocks/finalized",
                        web::get().to(move || {
                            let signed_block = signed_block.clone();
                            async move {
                                HttpResponse::Ok()
                                    .content_type("application/octet-stream")
                                    .body(signed_block.as_ssz_bytes())
                            }
                        }),
                    ),
            )
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        (
            Url::parse(&format!("http://{address}")).expect("Failed to parse base URL"),
            server_handle,
        )
    }

    #[tokio::test]
    async fn test_client_fetches_finalized_signed_block() {
        let state = make_state(10);
        let expected_signed_block = make_anchor_signed_block(&state);

        let (base_url, server_handle) =
            spawn_anchor_server(state.clone(), expected_signed_block.clone());

        let signed_block = LeanCheckpointClient::new()
            .fetch_finalized_block(&base_url)
            .await
            .expect("Client failed to fetch finalized signed block");

        assert_eq!(signed_block, expected_signed_block);

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_finalized_block_returns_http_error_context() {
        // Spawn a server that returns 404 on /blocks/finalized but a valid state.
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let address = listener.local_addr().expect("Failed to get local address");

        let server = HttpServer::new(move || {
            App::new().service(web::scope("/lean/v0").route(
                "/blocks/finalized",
                web::get().to(|| async {
                    HttpResponse::NotFound().body("anchor block missing".to_string())
                }),
            ))
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        let base_url = Url::parse(&format!("http://{address}")).expect("Failed to parse base URL");

        let err = LeanCheckpointClient::new()
            .fetch_finalized_block(&base_url)
            .await
            .expect_err("Expected finalized block fetch to fail");

        let err = err.to_string();
        assert!(err.contains("HTTP error 404"));
        assert!(err.contains("anchor block missing"));

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_finalized_block_returns_decode_error_for_malformed_ssz() {
        // Spawn a server that returns garbage bytes on /blocks/finalized.
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let address = listener.local_addr().expect("Failed to get local address");

        let server = HttpServer::new(move || {
            App::new().service(web::scope("/lean/v0").route(
                "/blocks/finalized",
                web::get().to(|| async {
                    HttpResponse::Ok()
                        .content_type("application/octet-stream")
                        .body(vec![0xffu8, 0xfe, 0x00])
                }),
            ))
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        let base_url = Url::parse(&format!("http://{address}")).expect("Failed to parse base URL");

        let err = LeanCheckpointClient::new()
            .fetch_finalized_block(&base_url)
            .await
            .expect_err("Expected SignedBlock decode to fail on garbage bytes");

        assert!(err.to_string().contains("SSZ decode failed"));

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_fetches_anchor_pair() {
        let state = make_state(10);
        let expected_signed_block = make_anchor_signed_block(&state);

        let (base_url, server_handle) =
            spawn_anchor_server(state.clone(), expected_signed_block.clone());

        let (fetched_state, fetched_signed_block) = LeanCheckpointClient::new()
            .fetch_finalized_anchor(&base_url)
            .await
            .expect("Client failed to fetch anchor pair");

        assert_eq!(fetched_state, state);
        assert_eq!(fetched_signed_block, expected_signed_block);
        assert_eq!(
            fetched_signed_block.block.state_root,
            fetched_state.tree_hash_root()
        );

        server_handle.stop(true).await;
    }

    #[tokio::test]
    async fn test_client_anchor_pair_rejects_state_root_mismatch() {
        // Simulates a server that advanced finalization between the state and
        // block fetches: the served block's `state_root` does not match
        // `hash_tree_root(state)`.
        let state = make_state(10);
        let mut mismatched_signed_block = make_anchor_signed_block(&state);
        mismatched_signed_block.block.state_root = B256::repeat_byte(0x01);

        let (base_url, server_handle) =
            spawn_anchor_server(state.clone(), mismatched_signed_block.clone());

        let err = LeanCheckpointClient::new()
            .fetch_finalized_anchor(&base_url)
            .await
            .expect_err("Expected mismatched anchor pair to fail");

        assert!(
            err.to_string().contains("Anchor block / state mismatch"),
            "unexpected error: {err}"
        );

        server_handle.stop(true).await;
    }
}
