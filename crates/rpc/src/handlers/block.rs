use std::collections::BTreeSet;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use alloy_primitives::B256;
use ream_consensus::{
    attester_slashing::AttesterSlashing,
    constants::{
        EFFECTIVE_BALANCE_INCREMENT, PROPOSER_WEIGHT, SLOTS_PER_EPOCH, SYNC_COMMITTEE_SIZE,
        SYNC_REWARD_WEIGHT, WEIGHT_DENOMINATOR, WHISTLEBLOWER_REWARD_QUOTIENT,
    },
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
};
use ream_network_spec::networks::NetworkSpec;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::types::{
    errors::ApiError,
    id::ID,
    response::{BeaconResponse, BeaconVersionedResponse, DataResponse, RootResponse},
};

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

pub async fn get_block_root_from_id(block_id: ID, db: &ReamDB) -> Result<B256, ApiError> {
    let block_root = match block_id {
        ID::Finalized => {
            let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|err| {
                error!("Failed to get block by block_root, error: {err:?}");
                ApiError::InternalError
            })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db.justified_checkpoint_provider().get().map_err(|err| {
                error!("Failed to get block by block_root, error: {err:?}");
                ApiError::InternalError
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
        error!("Failed to get block by block_root, error: {err:?}");
        ApiError::InternalError
    })?
    .ok_or_else(|| ApiError::NotFound(format!("Failed to find `block_root` from {block_id:?}")))?;

    Ok(block_root)
}

async fn get_beacon_state(block_id: ID, db: &ReamDB) -> Result<BeaconState, ApiError> {
    let block_root = get_block_root_from_id(block_id, db).await?;

    db.beacon_state_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon_state` from {block_root:?}"
        )))
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

fn get_sync_committee_rewards(beacon_state: &BeaconState, beacon_block: &SignedBeaconBlock) -> u64 {
    let total_active_balance = beacon_state.get_total_active_balance();
    let total_active_increments = total_active_balance / EFFECTIVE_BALANCE_INCREMENT;
    let total_base_rewards = beacon_state.get_base_reward_per_increment() * total_active_increments;
    let max_participant_rewards =
        total_base_rewards * SYNC_REWARD_WEIGHT / WEIGHT_DENOMINATOR / SLOTS_PER_EPOCH;
    let participant_reward = max_participant_rewards / SYNC_COMMITTEE_SIZE;
    let proposer_reward =
        participant_reward * PROPOSER_WEIGHT / (WEIGHT_DENOMINATOR - PROPOSER_WEIGHT);

    beacon_block
        .message
        .body
        .sync_aggregate
        .sync_committee_bits
        .num_set_bits() as u64
        * proposer_reward
}

fn get_slashable_attester_indices(
    beacon_state: &BeaconState,
    attester_shashing: &AttesterSlashing,
) -> Vec<u64> {
    let attestation_1 = &attester_shashing.attestation_1;
    let attestation_2 = &attester_shashing.attestation_2;

    let attestation_indices_1 = attestation_1
        .attesting_indices
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let attestation_indices_2 = attestation_2
        .attesting_indices
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut slashing_indices = vec![];

    for index in &attestation_indices_1 & &attestation_indices_2 {
        let validator = &beacon_state.validators[index as usize];
        let current_epoch = beacon_state.get_current_epoch();
        if validator.is_slashable_validator(current_epoch) {
            slashing_indices.push(index);
        }
    }

    slashing_indices
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
    for attester_shashing in attester_shashings {
        for index in get_slashable_attester_indices(beacon_state, attester_shashing) {
            let reward = beacon_state.validators[index as usize].effective_balance
                / WHISTLEBLOWER_REWARD_QUOTIENT;
            attester_slashing_reward += reward;
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
            error!("Failed to get block by block_root, error: {err:?}");
            ApiError::InternalError
        })?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Failed to find `beacon block` from {block_root:?}"))
        })
}

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
#[get("/beacon/genesis")]
pub async fn get_genesis(network_spec: Data<NetworkSpec>) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataResponse::new(network_spec.genesis.clone())))
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
    let beacon_state = get_beacon_state(block_id_value.clone(), &db).await?;

    let attestation_reward = get_attestations_rewards(&beacon_state, &beacon_block);
    let attester_slashing_reward = get_attester_slashing_rewards(&beacon_state, &beacon_block);
    let proposer_slashing_reward = get_proposer_slashing_rewards(&beacon_state, &beacon_block);
    let sync_committee_reward = get_sync_committee_rewards(&beacon_state, &beacon_block);

    let total = attestation_reward
        + sync_committee_reward
        + proposer_slashing_reward
        + attester_slashing_reward;

    let response = BlockRewards {
        proposer_index: beacon_block.message.proposer_index,
        total,
        attestations: attestation_reward,
        sync_aggregate: sync_committee_reward,
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
