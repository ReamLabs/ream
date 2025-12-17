use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder, get, post,
    web::{Data, Json, Path, Payload, Query},
};
use alloy_primitives::B256;
use futures::StreamExt;
use ream_api_types_beacon::{
    block::BroadcastValidation,
    id::ValidatorID,
    responses::{
        BeaconResponse, BeaconVersionedResponse, DataResponse, ETH_CONSENSUS_VERSION_HEADER,
        RootResponse, SSZ_CONTENT_TYPE,
    },
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    genesis::Genesis,
};
use ream_consensus_misc::constants::beacon::{
    WHISTLEBLOWER_REWARD_QUOTIENT, genesis_validators_root,
};
use ream_network_manager::p2p_sender::P2PSender;
use ream_network_spec::networks::beacon_network_spec;
use ream_p2p::{
    gossipsub::beacon::topics::{GossipTopic, GossipTopicKind},
    network::beacon::channel::GossipMessage,
};
use ream_storage::{
    cache::{AddressSlotIdentifier, CachedDB},
    db::beacon::BeaconDB,
    tables::{field::REDBField, table::REDBTable},
};
use serde::{Deserialize, Serialize};
use ssz::{Decode, Encode};
use tracing::{error, warn};
use tree_hash::TreeHash;

use crate::handlers::state::get_state_from_id;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BlockRewards {
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub total: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub attestations: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub sync_aggregate: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_slashings: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub attester_slashings: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatorSyncCommitteeReward {
    #[serde(with = "serde_utils::quoted_u64")]
    pub validator_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub reward: u64,
}

pub async fn get_block_root_from_id(block_id: ID, db: &BeaconDB) -> Result<B256, ApiError> {
    let block_root = match block_id {
        ID::Finalized => {
            let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get block by block_root, error: {err:?}"
                ))
            })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db.justified_checkpoint_provider().get().map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get block by block_root, error: {err:?}"
                ))
            })?;

            Ok(Some(justified_checkpoint.root))
        }
        ID::Head | ID::Genesis => {
            return Err(ApiError::NotFound(format!(
                "This ID type is currently not supported: {block_id:?}"
            )));
        }
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => Ok(Some(root)),
    }
    .map_err(|err| {
        ApiError::InternalError(format!("Failed to get block by block_root, error: {err:?}"))
    })?
    .ok_or_else(|| ApiError::NotFound(format!("Failed to find `block_root` from {block_id:?}")))?;

    Ok(block_root)
}

fn get_attestations_rewards(beacon_state: &BeaconState, beacon_block: &SignedBeaconBlock) -> u64 {
    let mut attester_reward = 0;
    let attestations = &beacon_block.message.body.attestations;
    for attestation in attestations {
        if let Ok(attesting_indices) = beacon_state.get_attesting_indices(attestation) {
            for index in attesting_indices {
                attester_reward += beacon_state.get_proposer_reward(index);
            }
        }
    }
    attester_reward
}

fn get_proposer_slashing_rewards(
    beacon_state: &BeaconState,
    beacon_block: &SignedBeaconBlock,
) -> u64 {
    let mut proposer_slashing_reward = 0;
    let proposer_slashings = &beacon_block.message.body.proposer_slashings;
    for proposer_slashing in proposer_slashings {
        let index = proposer_slashing.signed_header_1.message.proposer_index;
        let reward = beacon_state.validators[index as usize].effective_balance;
        proposer_slashing_reward += reward;
    }
    proposer_slashing_reward
}

fn get_attester_slashing_rewards(
    beacon_state: &BeaconState,
    beacon_block: &SignedBeaconBlock,
) -> u64 {
    let mut attester_slashing_reward = 0;
    let attester_shashings = &beacon_block.message.body.attester_slashings;
    let current_epoch = beacon_state.get_current_epoch();

    for attester_shashing in attester_shashings {
        if let Ok((attestation_indices_1, attestation_indices_2)) =
            beacon_state.get_slashable_attester_indices(attester_shashing)
        {
            for index in &attestation_indices_1 & &attestation_indices_2 {
                let validator = &beacon_state.validators[index as usize];
                if validator.is_slashable_validator(current_epoch) {
                    let reward = beacon_state.validators[index as usize].effective_balance
                        / WHISTLEBLOWER_REWARD_QUOTIENT;
                    attester_slashing_reward += reward;
                }
            }
        }
    }

    attester_slashing_reward
}

pub async fn get_beacon_block_from_id(
    block_id: ID,
    db: &BeaconDB,
) -> Result<SignedBeaconBlock, ApiError> {
    let block_root = get_block_root_from_id(block_id, db).await?;

    db.block_provider()
        .get(block_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get block by block_root, error: {err:?}"))
        })?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Failed to find `beacon block` from {block_root:?}"))
        })
}

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
#[get("/beacon/genesis")]
pub async fn get_genesis() -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataResponse::new(Genesis {
        genesis_time: beacon_network_spec().min_genesis_time,
        genesis_validators_root: genesis_validators_root(),
        genesis_fork_version: beacon_network_spec().genesis_fork_version,
    })))
}

/// Called by `/eth/v2/beacon/blocks/{block_id}/attestations` to get block attestations
#[get("/beacon/blocks/{block_id}/attestations")]
pub async fn get_block_attestations(
    db: Data<BeaconDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let beacon_block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(
        beacon_block.message.body.attestations,
    )))
}

/// Called by `/blocks/<block_id>/root` to get the Tree hash of the Block.
#[get("/beacon/blocks/{block_id}/root")]
pub async fn get_block_root(
    db: Data<BeaconDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let block_root = get_block_root_from_id(block_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconResponse::new(RootResponse::new(block_root))))
}

/// Called by `/beacon/blocks/{block_id}/rewards` to get the block rewards response
#[get("/beacon/blocks/{block_id}/rewards")]
pub async fn get_block_rewards(
    db: Data<BeaconDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let block_id_value = block_id.into_inner();
    let beacon_block = get_beacon_block_from_id(block_id_value.clone(), &db).await?;
    let beacon_state = get_state_from_id(block_id_value.clone(), &db).await?;

    let attestation_reward = get_attestations_rewards(&beacon_state, &beacon_block);
    let attester_slashing_reward = get_attester_slashing_rewards(&beacon_state, &beacon_block);
    let proposer_slashing_reward = get_proposer_slashing_rewards(&beacon_state, &beacon_block);
    let (_, proposer_reward) = beacon_state.get_proposer_and_participant_rewards();

    let sync_aggregate_reward = beacon_block
        .message
        .body
        .sync_aggregate
        .sync_committee_bits
        .num_set_bits() as u64
        * proposer_reward;

    let total = attestation_reward
        + sync_aggregate_reward
        + proposer_slashing_reward
        + attester_slashing_reward;

    let response = BlockRewards {
        proposer_index: beacon_block.message.proposer_index,
        total,
        attestations: attestation_reward,
        sync_aggregate: sync_aggregate_reward,
        proposer_slashings: proposer_slashing_reward,
        attester_slashings: attester_slashing_reward,
    };

    Ok(HttpResponse::Ok().json(BeaconResponse::new(response)))
}

/// Called by `/blocks/<block_id>` to get the Beacon Block.
#[get("/beacon/blocks/{block_id}")]
pub async fn get_block_from_id(
    db: Data<BeaconDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let beacon_block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(beacon_block)))
}

#[post("/beacon/rewards/sync_committee/{block_id}")]
pub async fn post_sync_committee_rewards(
    db: Data<BeaconDB>,
    block_id: Path<ID>,
    validators: Json<Vec<ValidatorID>>,
) -> Result<impl Responder, ApiError> {
    let block_id_value = block_id.into_inner();
    let beacon_block = get_beacon_block_from_id(block_id_value.clone(), &db).await?;
    let beacon_state = get_state_from_id(block_id_value.clone(), &db).await?;

    let sync_committee_rewards_map =
        match beacon_state.compute_sync_committee_rewards(&beacon_block) {
            Ok(rewards) => rewards,
            Err(err) => {
                error!("Failed to compute sync committee rewards, error: {err:?}");
                return Err(ApiError::InternalError(format!(
                    "Failed to compute sync committee rewards, error: {err:?}"
                )));
            }
        };
    let sync_committee_rewards: Vec<ValidatorSyncCommitteeReward> = sync_committee_rewards_map
        .into_iter()
        .map(|(validator_index, reward)| ValidatorSyncCommitteeReward {
            validator_index,
            reward,
        })
        .collect();

    let reward_data = if sync_committee_rewards.is_empty() {
        None
    } else if validators.is_empty() {
        Some(sync_committee_rewards)
    } else {
        Some(
            sync_committee_rewards
                .into_iter()
                .filter(|reward| {
                    validators.iter().any(|validator| match validator {
                        ValidatorID::Index(index) => *index == reward.validator_index,
                        ValidatorID::Address(pubkey) => {
                            match beacon_state.validators.get(reward.validator_index as usize) {
                                Some(validator) => validator.public_key == *pubkey,
                                None => false,
                            }
                        }
                    })
                })
                .collect::<Vec<ValidatorSyncCommitteeReward>>(),
        )
    };

    Ok(HttpResponse::Ok().json(BeaconResponse::new(reward_data)))
}

#[get("/beacon/blind_block/{block_id}")]
pub async fn get_blind_block(
    http_request: HttpRequest,
    db: Data<BeaconDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let beacon_block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;
    let blinded_beacon_block = beacon_block.as_signed_blinded_beacon_block();
    match http_request
        .headers()
        .get(SSZ_CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
    {
        Some(SSZ_CONTENT_TYPE) => Ok(HttpResponse::Ok()
            .content_type(SSZ_CONTENT_TYPE)
            .body(blinded_beacon_block.as_ssz_bytes())),
        _ => Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(blinded_beacon_block))),
    }
}

#[derive(Debug, Deserialize)]
pub struct BroadcastValidationQuery {
    #[serde(default)]
    pub broadcast_validation: BroadcastValidation,
}

/// POST /eth/v2/beacon/blocks
/// Publishes a signed beacon block to the beacon network
#[post("/beacon/blocks")]
pub async fn post_beacon_block(
    http_request: HttpRequest,
    payload: Payload,
    query: Query<BroadcastValidationQuery>,
    _db: Data<BeaconDB>,
    beacon_chain: Data<Arc<BeaconChain>>,
    p2p_sender: Data<Arc<P2PSender>>,
    cached_db: Data<Arc<CachedDB>>,
) -> Result<impl Responder, ApiError> {
    // 1. Validate Eth-Consensus-Version header
    let consensus_version = http_request
        .headers()
        .get(ETH_CONSENSUS_VERSION_HEADER)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "Missing required header: {ETH_CONSENSUS_VERSION_HEADER}"
            ))
        })?;

    let valid_versions = [
        "phase0",
        "altair",
        "bellatrix",
        "capella",
        "deneb",
        "electra",
        "fulu",
    ];
    if !valid_versions.contains(&consensus_version) {
        let valid_list = valid_versions.join(", ");
        return Err(ApiError::BadRequest(format!(
            "Invalid consensus version: {consensus_version}. Must be one of: {valid_list}"
        )));
    }

    // 2. Read SSZ payload
    let mut body = actix_web::web::BytesMut::new();
    let mut stream = payload.into_inner();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|err| ApiError::BadRequest(format!("Failed to read request body: {err}")))?;
        body.extend_from_slice(&chunk);
    }

    let signed_block = SignedBeaconBlock::from_ssz_bytes(&body.freeze())
        .map_err(|err| ApiError::BadRequest(format!("Failed to decode SSZ block: {err:?}")))?;

    // 3. Validate based on broadcast_validation level
    validate_block_for_broadcast(
        beacon_chain.as_ref(),
        &signed_block,
        &query.broadcast_validation,
        cached_db.as_ref(),
    )
    .await
    .map_err(|err| ApiError::BadRequest(format!("Block validation failed: {err}")))?;

    // 4. Broadcast via P2P
    let fork_digest = beacon_network_spec().fork_digest(genesis_validators_root());
    let topic = GossipTopic {
        fork: fork_digest,
        kind: GossipTopicKind::BeaconBlock,
    };

    p2p_sender.send_gossip(GossipMessage {
        topic,
        data: signed_block.as_ssz_bytes(),
    });

    // 5. Integrate into state (after broadcast)
    let integration_success = match beacon_chain.process_block(signed_block.clone()).await {
        Ok(()) => true,
        Err(err) => {
            // Check if block is already known - this is not an error
            if err.to_string().contains("already known")
                || err.to_string().contains("ALREADY_KNOWN")
            {
                warn!("Block already known, ignoring: {}", err);
                return Ok(HttpResponse::Ok().finish());
            }
            error!("Failed to integrate block into state: {}", err);
            false
        }
    };

    // 6. Return appropriate status
    if integration_success {
        Ok(HttpResponse::Ok().finish())
    } else {
        // 202: validation passed, broadcast succeeded, but integration failed
        Ok(HttpResponse::Accepted().finish())
    }
}

async fn validate_block_for_broadcast(
    beacon_chain: &BeaconChain,
    block: &SignedBeaconBlock,
    validation_level: &BroadcastValidation,
    cached_db: &CachedDB,
) -> Result<(), String> {
    match validation_level {
        BroadcastValidation::Gossip => {
            // Lightweight gossip validation
            validate_gossip_level(beacon_chain, block, cached_db).await
        }
        BroadcastValidation::Consensus => {
            // Full consensus validation
            validate_consensus_level(beacon_chain, block, cached_db).await
        }
        BroadcastValidation::ConsensusAndEquivocation => {
            // Consensus + equivocation check
            validate_consensus_level(beacon_chain, block, cached_db).await?;
            check_equivocation(beacon_chain, block).await
        }
    }
}

async fn validate_gossip_level(
    beacon_chain: &BeaconChain,
    block: &SignedBeaconBlock,
    cached_db: &CachedDB,
) -> Result<(), String> {
    let store = beacon_chain.store.lock().await;

    // Check slot not in future
    let current_slot = store
        .get_current_slot()
        .map_err(|err| format!("Failed to get current slot: {err}"))?;
    if block.message.slot > current_slot {
        return Err("Block is from a future slot".to_string());
    }

    // Check parent exists
    let parent_state = store
        .db
        .state_provider()
        .get(block.message.parent_root)
        .map_err(|err| format!("Failed to get parent state: {err}"))?
        .ok_or_else(|| "Parent state not found".to_string())?;

    // Verify signature
    if !parent_state
        .verify_block_header_signature(&block.signed_header())
        .map_err(|err| format!("Signature verification error: {err}"))?
    {
        return Err("Invalid block signature".to_string());
    }

    // Check for duplicate (using CachedDB)
    let validator = parent_state
        .validators
        .get(block.message.proposer_index as usize)
        .ok_or_else(|| "Validator not found".to_string())?;

    if cached_db
        .seen_proposer_signature
        .read()
        .await
        .contains(&AddressSlotIdentifier {
            address: validator.public_key.clone(),
            slot: block.message.slot,
        })
    {
        return Err("Block already seen from this proposer".to_string());
    }

    Ok(())
}

async fn validate_consensus_level(
    beacon_chain: &BeaconChain,
    block: &SignedBeaconBlock,
    cached_db: &CachedDB,
) -> Result<(), String> {
    // First do gossip checks
    validate_gossip_level(beacon_chain, block, cached_db).await?;

    let store = beacon_chain.store.lock().await;
    let state = store
        .db
        .state_provider()
        .get(block.message.parent_root)
        .map_err(|err| format!("Failed to get parent state: {err}"))?
        .ok_or_else(|| "Parent state not found".to_string())?;

    // Check proposer index
    let expected_proposer = state
        .get_beacon_proposer_index(Some(block.message.slot))
        .map_err(|err| format!("Failed to get proposer index: {err}"))?;
    if expected_proposer != block.message.proposer_index {
        let got = block.message.proposer_index;
        return Err(format!(
            "Invalid proposer index: expected {expected_proposer}, got {got}"
        ));
    }

    // Check execution payload timestamp
    let expected_timestamp = state.compute_timestamp_at_slot(block.message.slot);
    if block.message.body.execution_payload.timestamp != expected_timestamp {
        let got = block.message.body.execution_payload.timestamp;
        return Err(format!(
            "Invalid execution payload timestamp: expected {expected_timestamp}, got {got}"
        ));
    }

    Ok(())
}

async fn check_equivocation(
    beacon_chain: &BeaconChain,
    block: &SignedBeaconBlock,
) -> Result<(), String> {
    let store = beacon_chain.store.lock().await;
    let block_root = block.message.tree_hash_root();

    // Check if another block exists with same slot/proposer but different root
    // This is simplified - production would need more sophisticated tracking
    if let Ok(Some(existing_block)) = store.db.block_provider().get(block_root)
        && existing_block.message.slot == block.message.slot
        && existing_block.message.proposer_index == block.message.proposer_index
        && existing_block.message.tree_hash_root() != block_root
    {
        return Err("Equivocation detected".to_string());
    }

    Ok(())
}
