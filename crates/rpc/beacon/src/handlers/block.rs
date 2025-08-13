use actix_web::{
    HttpRequest, HttpResponse, Responder, get, post,
    web::{Data, Json, Path},
};
use alloy_primitives::B256;
use ream_api_types_beacon::{
    error::ApiError,
    id::{ID, ValidatorID},
    responses::{
        BeaconResponse, BeaconVersionedResponse, DataResponse, RootResponse, SSZ_CONTENT_TYPE,
    },
};
use ream_consensus_beacon::{
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    genesis::Genesis,
};
use ream_consensus_misc::constants::beacon::{
    WHISTLEBLOWER_REWARD_QUOTIENT, genesis_validators_root,
};
use ream_network_spec::networks::beacon_network_spec;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use serde::{Deserialize, Serialize};
use ssz::Encode;
use tracing::error;

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

pub async fn get_block_root_from_id(block_id: ID, db: &ReamDB) -> Result<B256, ApiError> {
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
    db: &ReamDB,
) -> Result<SignedBeaconBlock, ApiError> {
    let block_root = get_block_root_from_id(block_id, db).await?;

    db.beacon_block_provider()
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
    db: Data<ReamDB>,
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
    db: Data<ReamDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let block_root = get_block_root_from_id(block_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconResponse::new(RootResponse::new(block_root))))
}

/// Called by `/beacon/blocks/{block_id}/rewards` to get the block rewards response
#[get("/beacon/blocks/{block_id}/rewards")]
pub async fn get_block_rewards(
    db: Data<ReamDB>,
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
    db: Data<ReamDB>,
    block_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let beacon_block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(beacon_block)))
}

#[post("/beacon/rewards/sync_committee/{block_id}")]
pub async fn post_sync_committee_rewards(
    db: Data<ReamDB>,
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
    db: Data<ReamDB>,
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
