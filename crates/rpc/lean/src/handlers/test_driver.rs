use std::{env, fmt::Display};

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use alloy_primitives::{B256, hex};
use anyhow::anyhow;
use lean_spec_tests::types::{
    Block as FixtureBlock, GossipAggregatedAttestationStep, State as FixtureState,
    fork_choice::ForkChoiceStep,
    ssz::{SignedBlockJSON, StateJSON},
};
use ream_api_types_common::error::ApiError;
use ream_consensus_lean::{
    attestation::{AggregatedSignatureProof, SignedAggregatedAttestation, SignedAttestation},
    block::{Block as ReamBlock, BlockBody, BlockSignatures, SignedBlock},
    state::LeanState,
    validator::Validator,
};
use ream_fork_choice_lean::{
    genesis::setup_genesis,
    store::{LeanStoreWriter, Store},
};
use ream_network_spec::networks::lean_network_spec;
use ream_post_quantum_crypto::leansig::{public_key::PublicKey, signature::Signature};
use ream_storage::{
    db::ReamDB,
    dir::setup_data_dir,
    tables::{field::REDBField, table::REDBTable},
};
use serde::{Deserialize, Serialize};
use ssz_types::{
    BitList, VariableList,
    typenum::{U4096, U1048576},
};
use tree_hash::TreeHash;

const TEST_DRIVER_ENV: &str = "HIVE_LEAN_TEST_DRIVER";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkChoiceInitRequest {
    anchor_state: FixtureState,
    anchor_block: FixtureBlock,
    #[serde(default)]
    genesis_time: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetRequest {
    #[serde(default)]
    genesis_params: Option<GenesisParams>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisParams {
    #[serde(default)]
    num_validators: Option<usize>,
    #[serde(default)]
    genesis_time: Option<u64>,
    #[serde(default)]
    anchor_slot: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTransitionRunRequest {
    pre: FixtureState,
    blocks: Vec<FixtureBlock>,
    #[serde(default)]
    expect_exception: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySignaturesRunRequest {
    anchor_state: StateJSON,
    signed_block: SignedBlockJSON,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestDriverCheckpoint {
    slot: u64,
    root: B256,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkChoiceSnapshot {
    head_slot: u64,
    head_root: B256,
    time: u64,
    justified_checkpoint: TestDriverCheckpoint,
    finalized_checkpoint: TestDriverCheckpoint,
    safe_target: B256,
    validator_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkChoiceStepResponse {
    accepted: bool,
    error: Option<String>,
    snapshot: ForkChoiceSnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTransitionPostSummary {
    slot: u64,
    latest_block_header_slot: u64,
    latest_block_header_state_root: B256,
    historical_block_hashes_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTransitionRunResponse {
    succeeded: bool,
    error: Option<String>,
    post: Option<StateTransitionPostSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySignaturesRunResponse {
    succeeded: bool,
    error: Option<String>,
}

pub fn test_driver_enabled() -> bool {
    env::var(TEST_DRIVER_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

impl StateTransitionPostSummary {
    fn from_state(state: &LeanState) -> Self {
        Self {
            slot: state.slot,
            latest_block_header_slot: state.latest_block_header.slot,
            latest_block_header_state_root: state.latest_block_header.state_root,
            historical_block_hashes_count: state.historical_block_hashes.len(),
        }
    }
}

impl StateTransitionRunResponse {
    fn success(state: &LeanState) -> Self {
        Self {
            succeeded: true,
            error: None,
            post: Some(StateTransitionPostSummary::from_state(state)),
        }
    }

    fn error(err: impl Display) -> Self {
        Self {
            succeeded: false,
            error: Some(err.to_string()),
            post: None,
        }
    }
}

impl VerifySignaturesRunResponse {
    fn success() -> Self {
        Self {
            succeeded: true,
            error: None,
        }
    }

    fn error(err: impl Display) -> Self {
        Self {
            succeeded: false,
            error: Some(err.to_string()),
        }
    }
}

fn driver_error(context: impl Into<String>, err: impl Display) -> ApiError {
    ApiError::InternalError(format!("{}: {err}", context.into()))
}

fn new_test_db() -> Result<ream_storage::db::lean::LeanDB, ApiError> {
    let dir = setup_data_dir("hive_lean_test_driver", None, true)
        .map_err(|err| driver_error("failed to create test-driver data dir", err))?;
    ReamDB::new(dir)
        .map_err(|err| driver_error("failed to create test-driver ReamDB", err))?
        .init_lean_db()
        .map_err(|err| driver_error("failed to initialize test-driver LeanDB", err))
}

fn blank_signed_block(block: ReamBlock) -> anyhow::Result<SignedBlock> {
    let proofs = block
        .body
        .attestations
        .iter()
        .map(|attestation| {
            Ok(AggregatedSignatureProof::new(
                attestation.aggregation_bits.clone(),
                VariableList::<u8, U1048576>::new(vec![])
                    .map_err(|err| anyhow!("failed to build empty proof data: {err:?}"))?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(SignedBlock {
        block,
        signature: BlockSignatures {
            attestation_signatures: VariableList::try_from(proofs)
                .map_err(|err| anyhow!("failed to build attestation signatures list: {err}"))?,
            proposer_signature: Signature::blank(),
        },
    })
}

fn parse_signature(signature: Option<&str>) -> anyhow::Result<Signature> {
    signature
        .map(|value| {
            let bytes = hex::decode(value.trim_start_matches("0x"))
                .map_err(|err| anyhow!("failed to decode signature hex: {err}"))?;
            Ok(Signature::from(bytes.as_slice()))
        })
        .transpose()
        .map(|signature| signature.unwrap_or_else(Signature::blank))
}

fn test_validators(count: usize) -> Result<Vec<Validator>, ApiError> {
    const TEST_KEYS: [(&str, &str); 8] = [
        (
            "80a6f13b39b9c26cd91edb542bdb9e051b61223a15f5de18e53e6a361720cd65c06a114e87531241cc535f39156f0f2cf0877647",
            "dedd8e5a890a2c339ffbca1ce85b710384670508df73ce33116bcd60befbef5a560e6304e3635a58e12b6b542c1bc6583a16455a",
        ),
        (
            "7612d36fb7667d1c2f19a821d6a63b3cc17f214baedecb57e6eed4600dabf055185de12c6418db6b79f71004303bfe3a753c9727",
            "1dcca64f8f9956470c2a427ca669d9359f22f202387ba24440e77200ec57983515451279124ee834a62f77560b1c166b0eb2885b",
        ),
        (
            "d703e67ce93c9b61b2ae88063ce60f13822efe03d8f089582615032e2b44346fa0199c2320dca35012ec1d414dc3b835da11cd73",
            "9ee9237afcc3d75c0a8271412f2ac10f03c0da760e545a21cc7eb8331381f62bec027d1dac093b7a09057c187dd46e6231c0c007",
        ),
        (
            "e41f343f892528158dccae5404bd7262106386597fd4a35ae2c682374c6f15706dd8062ddac61b1851020e0918e7970917e30b06",
            "57ffb567b826fb60dd07da58f6939b05aa93cd4bb2d9794308d3232878caa5076e9628292e96406c2d8faa3f8108922cdcf37e56",
        ),
        (
            "8276347211a4c768a823454b927ee07a8895a24ccf0a7d2f08b298122476c746d4df7c63754e521d62343044ff48e30bb090e472",
            "3a527d4a67783d7ae9077b250565370cf6c5f34e5f038362a9308f414cb68657fa888a161182dd0b4456771885a2b82861fe170c",
        ),
        (
            "3e956459bc3031258823bc4cf7e24f3c00cc7341a633d138997149635cf7096f5bf13468a9c5be7c67ab357b62724446819b1513",
            "92614a0d3a3c1801cce44829ba31ad3738c748509278b32b384561072216840f218791726fc46a56ca2fff3802e4852ad975e730",
        ),
        (
            "eb3a4219238a832b42395107025de92b78534b615f717b2e657b1615d53eef16e00ff86909c67d60a1b57f3c938fb27369973a09",
            "b9dacc060827fa328d44b65461fc74305f6586664e83f8436f435754df6bd5060c570e5985ca6b779213f717bb57742757a9cb75",
        ),
        (
            "b943ea3a05d88c48bb793031dd29a90b922ea654b2a7ed55ea5956396cb951321a24987c07bee00ff4c56823809bac214472884e",
            "cf02f74f4e96a556ad3c90422cb7e874c88e5d365dedf754178f74110247ec7d39f0df2b9385cb0b0d837278fb64572b1419b05d",
        ),
    ];

    if count > TEST_KEYS.len() {
        return Err(ApiError::BadRequest(format!(
            "test-driver reset supports at most {} validators, got {count}",
            TEST_KEYS.len()
        )));
    }

    TEST_KEYS
        .iter()
        .take(count)
        .enumerate()
        .map(|(index, (attestation, proposal))| {
            let attestation_bytes = hex::decode(attestation)
                .map_err(|err| driver_error("failed to decode attestation pubkey", err))?;
            let proposal_bytes = hex::decode(proposal)
                .map_err(|err| driver_error("failed to decode proposal pubkey", err))?;
            Ok(Validator {
                attestation_public_key: PublicKey::from(attestation_bytes.as_slice()),
                proposal_public_key: PublicKey::from(proposal_bytes.as_slice()),
                index: index as u64,
            })
        })
        .collect()
}

async fn build_genesis_store(params: Option<&GenesisParams>) -> Result<Store, ApiError> {
    let num_validators = params.and_then(|params| params.num_validators).unwrap_or(4);
    let genesis_time = params
        .and_then(|params| params.genesis_time)
        .unwrap_or_default();
    let anchor_slot = params
        .and_then(|params| params.anchor_slot)
        .unwrap_or_default();

    if num_validators == 0 && anchor_slot > 0 {
        return Err(ApiError::BadRequest(
            "cannot advance test-driver genesis with zero validators".to_string(),
        ));
    }

    let (mut anchor_block, mut anchor_state) =
        setup_genesis(genesis_time, test_validators(num_validators)?);

    for slot in 1..=anchor_slot {
        let proposer_index = slot % num_validators as u64;
        let parent_root = anchor_block.tree_hash_root();
        let mut post_state = anchor_state.clone();
        let mut block = ReamBlock {
            slot,
            proposer_index,
            parent_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };
        post_state
            .process_slots(slot)
            .map_err(|err| driver_error(format!("failed to process slots to {slot}"), err))?;
        post_state
            .process_block(&block)
            .map_err(|err| driver_error(format!("failed to build block at slot {slot}"), err))?;
        block.state_root = post_state.tree_hash_root();
        anchor_block = block;
        anchor_state = post_state;
    }

    let db = new_test_db()?;
    let signed_block = blank_signed_block(anchor_block)
        .map_err(|err| driver_error("failed to build signed anchor block", err))?;
    Store::get_forkchoice_store(signed_block, anchor_state, db, None, None)
        .map_err(|err| driver_error("failed to initialize fork-choice store", err))
}

fn convert_gossip_aggregate(
    fixture: &GossipAggregatedAttestationStep,
) -> anyhow::Result<SignedAggregatedAttestation> {
    let mut participants =
        BitList::<U4096>::with_capacity(fixture.proof.participants.data.len())
            .map_err(|err| anyhow!("failed to create participants bitlist: {err:?}"))?;
    for (index, &bit) in fixture.proof.participants.data.iter().enumerate() {
        participants
            .set(index, bit)
            .map_err(|err| anyhow!("failed to set participant bit {index}: {err:?}"))?;
    }

    let proof_bytes = hex::decode(fixture.proof.proof_data.data.trim_start_matches("0x"))
        .map_err(|err| anyhow!("failed to decode aggregate proof bytes: {err}"))?;
    let proof_data = VariableList::<u8, U1048576>::new(proof_bytes)
        .map_err(|err| anyhow!("failed to build proof data list: {err:?}"))?;

    Ok(SignedAggregatedAttestation {
        data: fixture.data.clone(),
        proof: AggregatedSignatureProof::new(participants, proof_data),
    })
}

async fn load_snapshot(writer: &LeanStoreWriter) -> Result<ForkChoiceSnapshot, ApiError> {
    let store = writer.read().await;
    let db = store.store.lock().await;
    let head_root = db
        .head_provider()
        .get()
        .map_err(|err| driver_error("failed to read head root", err))?;
    let head_block = db
        .block_provider()
        .get(head_root)
        .map_err(|err| driver_error("failed to read head block", err))?
        .ok_or_else(|| ApiError::InternalError("head block not found".to_string()))?;
    let justified = db
        .latest_justified_provider()
        .get()
        .map_err(|err| driver_error("failed to read justified checkpoint", err))?;
    let finalized = db
        .latest_finalized_provider()
        .get()
        .map_err(|err| driver_error("failed to read finalized checkpoint", err))?;
    let safe_target = db
        .safe_target_provider()
        .get()
        .map_err(|err| driver_error("failed to read safe target", err))?;
    let time = db
        .time_provider()
        .get()
        .map_err(|err| driver_error("failed to read store time", err))?;
    let validator_count = db
        .state_provider()
        .get(head_root)
        .map_err(|err| driver_error("failed to read head state", err))?
        .map(|state| state.validators.len() as u64)
        .unwrap_or(0);

    Ok(ForkChoiceSnapshot {
        head_slot: head_block.block.slot,
        head_root,
        time,
        justified_checkpoint: TestDriverCheckpoint {
            slot: justified.slot,
            root: justified.root,
        },
        finalized_checkpoint: TestDriverCheckpoint {
            slot: finalized.slot,
            root: finalized.root,
        },
        safe_target,
        validator_count,
    })
}

async fn advance_to_interval(
    store: &mut Store,
    target_interval: u64,
    has_proposal: bool,
    is_aggregator: bool,
) -> anyhow::Result<()> {
    loop {
        let current_interval = {
            let db = store.store.lock().await;
            db.time_provider().get()?
        };
        if current_interval >= target_interval {
            return Ok(());
        }

        store
            .tick_interval(
                has_proposal && current_interval + 1 == target_interval,
                is_aggregator,
            )
            .await?;
    }
}

async fn advance_to_block_slot(
    store: &mut Store,
    block: &ReamBlock,
    has_proposal: bool,
    is_aggregator: bool,
) -> anyhow::Result<()> {
    let genesis_time = {
        let db = store.store.lock().await;
        let head_root = db.head_provider().get()?;
        db.state_provider()
            .get(head_root)?
            .ok_or_else(|| anyhow!("head state not found while reading genesis time"))?
            .config
            .genesis_time
    };
    let slot_time = genesis_time + block.slot * lean_network_spec().seconds_per_slot;
    store.on_tick(slot_time, has_proposal, is_aggregator).await
}

async fn apply_fork_choice_step(store: &mut Store, step: ForkChoiceStep) -> anyhow::Result<()> {
    match step {
        ForkChoiceStep::Tick {
            time,
            interval,
            has_proposal,
            ..
        } => match (time, interval) {
            (Some(tick_time), _) => {
                store
                    .on_tick(tick_time, has_proposal.unwrap_or(false), true)
                    .await
            }
            (None, Some(target_interval)) => {
                advance_to_interval(store, target_interval, has_proposal.unwrap_or(false), true)
                    .await
            }
            (None, None) => Err(anyhow!("tick missing time or interval")),
        },
        ForkChoiceStep::Block { block, .. } => {
            let block = ReamBlock::try_from(&block)?;
            advance_to_block_slot(store, &block, true, true).await?;
            let signed_block = blank_signed_block(block)?;
            store.on_block(&signed_block, false).await
        }
        ForkChoiceStep::Attestation { attestation, .. } => {
            let signed = SignedAttestation {
                validator_id: attestation.validator_id,
                message: attestation.data.clone(),
                signature: parse_signature(attestation.signature.as_deref())?,
            };
            store.on_gossip_attestation(signed, false).await
        }
        ForkChoiceStep::GossipAggregatedAttestation { attestation, .. } => {
            if let Some(attestation) = attestation {
                store
                    .on_gossip_aggregated_attestation(convert_gossip_aggregate(&attestation)?)
                    .await
            } else {
                Ok(())
            }
        }
        ForkChoiceStep::Checks { .. } => Ok(()),
    }
}

fn apply_state_transition_blocks(
    state: &mut LeanState,
    blocks: &[FixtureBlock],
) -> anyhow::Result<()> {
    for block in blocks {
        let block =
            ReamBlock::try_from(block).map_err(|err| anyhow!("failed to convert block: {err}"))?;
        state
            .state_transition(&block, true)
            .map_err(|err| anyhow!("state transition failed: {err}"))?;
    }

    Ok(())
}

#[post("/test_driver/reset")]
pub async fn reset_store(
    request: Option<Json<ResetRequest>>,
    lean_chain: Data<LeanStoreWriter>,
) -> Result<impl Responder, ApiError> {
    let store = build_genesis_store(
        request
            .as_ref()
            .and_then(|request| request.genesis_params.as_ref()),
    )
    .await?;
    *lean_chain.write().await = store;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/test_driver/fork_choice/init")]
pub async fn init_fork_choice(
    request: Json<ForkChoiceInitRequest>,
    lean_chain: Data<LeanStoreWriter>,
) -> Result<impl Responder, ApiError> {
    let mut state = LeanState::try_from(request.anchor_state.clone())
        .map_err(|err| driver_error("failed to convert anchor state", err))?;
    if let Some(genesis_time) = request.genesis_time {
        state.config.genesis_time = genesis_time;
    }
    let block = ReamBlock::try_from(&request.anchor_block)
        .map_err(|err| driver_error("failed to convert anchor block", err))?;
    let signed_block = blank_signed_block(block)
        .map_err(|err| driver_error("failed to build signed anchor block", err))?;
    let db = new_test_db()?;
    let store = Store::get_forkchoice_store(signed_block, state, db, None, None)
        .map_err(|err| driver_error("failed to initialize fork-choice store", err))?;
    *lean_chain.write().await = store;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/test_driver/fork_choice/step")]
pub async fn step_fork_choice(
    request: Json<ForkChoiceStep>,
    lean_chain: Data<LeanStoreWriter>,
) -> Result<impl Responder, ApiError> {
    let result = {
        let mut store = lean_chain.write().await;
        apply_fork_choice_step(&mut store, request.into_inner()).await
    };
    let (accepted, error) = match result {
        Ok(()) => (true, None),
        Err(err) => (false, Some(err.to_string())),
    };

    let snapshot = load_snapshot(&lean_chain).await?;
    Ok(HttpResponse::Ok().json(ForkChoiceStepResponse {
        accepted,
        error,
        snapshot,
    }))
}

#[get("/test_driver/fork_choice/snapshot")]
pub async fn snapshot_fork_choice(
    lean_chain: Data<LeanStoreWriter>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(load_snapshot(&lean_chain).await?))
}

#[post("/test_driver/state_transition/run")]
pub async fn run_state_transition(
    request: Json<StateTransitionRunRequest>,
) -> Result<impl Responder, ApiError> {
    let mut state = LeanState::try_from(request.pre.clone())
        .map_err(|err| driver_error("failed to convert pre-state", err))?;

    let result = apply_state_transition_blocks(&mut state, &request.blocks).and_then(|()| {
        if request.blocks.is_empty() && request.expect_exception.is_some() {
            let target_slot = state.slot;
            state
                .process_slots(target_slot)
                .map_err(|err| anyhow!("process_slots({target_slot}) failed: {err}"))?;
        }

        Ok(())
    });

    match result {
        Ok(()) => Ok(HttpResponse::Ok().json(StateTransitionRunResponse::success(&state))),
        Err(err) => Ok(HttpResponse::Ok().json(StateTransitionRunResponse::error(err))),
    }
}

#[post("/test_driver/verify_signatures/run")]
pub async fn run_verify_signatures(
    request: Json<VerifySignaturesRunRequest>,
) -> Result<impl Responder, ApiError> {
    let result = (|| -> anyhow::Result<()> {
        let parent_state = LeanState::try_from(&request.anchor_state)
            .map_err(|err| anyhow!("failed to convert anchor state: {err}"))?;
        let signed_block = SignedBlock::try_from(&request.signed_block)
            .map_err(|err| anyhow!("failed to convert signed block: {err}"))?;
        signed_block
            .verify_signatures(&parent_state, true)
            .map(|_| ())
            .map_err(|err| anyhow!("verify_signatures failed: {err}"))
    })();

    match result {
        Ok(()) => Ok(HttpResponse::Ok().json(VerifySignaturesRunResponse::success())),
        Err(err) => Ok(HttpResponse::Ok().json(VerifySignaturesRunResponse::error(err))),
    }
}
