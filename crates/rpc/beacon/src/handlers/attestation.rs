use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Json, Path},
};
use ream_api_types_beacon::{
    id::ValidatorID,
    rewards::{AttestationRewardsData, AttestationRewardsResponse, IdealReward, TotalReward},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::constants::beacon::{
    EFFECTIVE_BALANCE_INCREMENT, GENESIS_EPOCH, PARTICIPATION_FLAG_WEIGHTS, SLOTS_PER_EPOCH,
    TIMELY_HEAD_FLAG_INDEX, TIMELY_SOURCE_FLAG_INDEX, TIMELY_TARGET_FLAG_INDEX, WEIGHT_DENOMINATOR,
};
use ream_storage::{db::beacon::BeaconDB, tables::field::REDBField};

use crate::handlers::state::get_state_from_id;

fn compute_ideal_attestation_rewards(state: &BeaconState) -> Result<Vec<IdealReward>, ApiError> {
    let base_reward_per_increment = state.get_base_reward_per_increment();
    let active_increments = state.get_total_active_balance() / EFFECTIVE_BALANCE_INCREMENT;
    let previous_epoch = state.get_previous_epoch();
    let is_inactivity_leak = state.is_in_inactivity_leak();

    // Get unslashed participating balances for each flag
    let unslashed_participating_indices_source = state
        .get_unslashed_participating_indices(TIMELY_SOURCE_FLAG_INDEX, previous_epoch)
        .map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to get source participating indices: {err:?}"
            ))
        })?;
    let unslashed_participating_indices_target = state
        .get_unslashed_participating_indices(TIMELY_TARGET_FLAG_INDEX, previous_epoch)
        .map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to get target participating indices: {err:?}"
            ))
        })?;
    let unslashed_participating_indices_head = state
        .get_unslashed_participating_indices(TIMELY_HEAD_FLAG_INDEX, previous_epoch)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get head participating indices: {err:?}"))
        })?;

    let unslashed_participating_increments_source = state
        .get_total_balance(unslashed_participating_indices_source)
        / EFFECTIVE_BALANCE_INCREMENT;
    let unslashed_participating_increments_target = state
        .get_total_balance(unslashed_participating_indices_target)
        / EFFECTIVE_BALANCE_INCREMENT;
    let unslashed_participating_increments_head =
        state.get_total_balance(unslashed_participating_indices_head) / EFFECTIVE_BALANCE_INCREMENT;

    // Collect all unique effective balances from validators
    let mut unique_effective_balances: Vec<u64> = state
        .validators
        .iter()
        .map(|v| v.effective_balance)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    unique_effective_balances.sort_unstable();

    // Calculate ideal rewards for each effective balance
    let mut ideal_rewards = Vec::new();
    for effective_balance in unique_effective_balances {
        let effective_balance_increments = effective_balance / EFFECTIVE_BALANCE_INCREMENT;
        let base_reward = effective_balance_increments * base_reward_per_increment;

        // Calculate ideal reward for each flag (assuming perfect participation, no inactivity leak)
        let ideal_source = if !is_inactivity_leak {
            let reward_numerator = base_reward
                * PARTICIPATION_FLAG_WEIGHTS[TIMELY_SOURCE_FLAG_INDEX as usize]
                * unslashed_participating_increments_source;
            reward_numerator / (active_increments * WEIGHT_DENOMINATOR)
        } else {
            0
        };

        let ideal_target = if !is_inactivity_leak {
            let reward_numerator = base_reward
                * PARTICIPATION_FLAG_WEIGHTS[TIMELY_TARGET_FLAG_INDEX as usize]
                * unslashed_participating_increments_target;
            reward_numerator / (active_increments * WEIGHT_DENOMINATOR)
        } else {
            0
        };

        let ideal_head = if !is_inactivity_leak {
            let reward_numerator = base_reward
                * PARTICIPATION_FLAG_WEIGHTS[TIMELY_HEAD_FLAG_INDEX as usize]
                * unslashed_participating_increments_head;
            reward_numerator / (active_increments * WEIGHT_DENOMINATOR)
        } else {
            0
        };

        ideal_rewards.push(IdealReward {
            effective_balance,
            head: ideal_head,
            target: ideal_target,
            source: ideal_source,
            // Inclusion delay was removed after Altair fork, always 0
            inclusion_delay: 0,
            // Ideal case assumes perfect participation, so no inactivity penalty
            inactivity: 0,
        });
    }

    Ok(ideal_rewards)
}

struct AttestationDeltas {
    source_rewards: Vec<u64>,
    source_penalties: Vec<u64>,
    target_rewards: Vec<u64>,
    target_penalties: Vec<u64>,
    head_rewards: Vec<u64>,
    head_penalties: Vec<u64>,
    inactivity_penalties: Vec<u64>,
}

fn compute_total_attestation_rewards(
    state: &BeaconState,
    deltas: &AttestationDeltas,
    validator_ids: Option<&Vec<ValidatorID>>,
) -> Result<Vec<TotalReward>, ApiError> {
    let eligible_indices = state.get_eligible_validator_indices().map_err(|err| {
        ApiError::InternalError(format!("Failed to get eligible validator indices: {err:?}"))
    })?;

    let mut total_rewards = Vec::new();

    for &validator_index in &eligible_indices {
        // Filter by validator_ids if provided
        if let Some(ids) = validator_ids
            && !ids.is_empty()
        {
            let matches = ids.iter().any(|id| match id {
                ValidatorID::Index(index) => *index == validator_index,
                ValidatorID::Address(pubkey) => {
                    if let Some(validator) = state.validators.get(validator_index as usize) {
                        validator.public_key == *pubkey
                    } else {
                        false
                    }
                }
            });
            if !matches {
                continue;
            }
        }

        let idx = validator_index as usize;

        // Calculate actual rewards/penalties for this validator
        let source = deltas.source_rewards[idx] as i64 - deltas.source_penalties[idx] as i64;
        let target = deltas.target_rewards[idx] as i64 - deltas.target_penalties[idx] as i64;
        let head = deltas.head_rewards[idx] as i64 - deltas.head_penalties[idx] as i64;
        let inactivity = -(deltas.inactivity_penalties[idx] as i64);

        total_rewards.push(TotalReward {
            validator_index,
            head,
            target,
            source,
            inclusion_delay: 0, // Inclusion delay removed after Altair fork
            inactivity,
        });
    }

    Ok(total_rewards)
}

#[post("/beacon/rewards/attestations/{epoch}")]
pub async fn post_attestation_rewards(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
    validator_ids: Json<Option<Vec<ValidatorID>>>,
) -> Result<impl Responder, ApiError> {
    let epoch_value = epoch.into_inner();

    // Genesis epoch has no rewards
    if epoch_value == GENESIS_EPOCH {
        return Err(ApiError::BadRequest(
            "No rewards are applied at the end of genesis epoch".to_string(),
        ));
    }

    // Using the last slot of epoch+1 to capture all attestations including late ones
    let target_slot = (epoch_value + 2) * SLOTS_PER_EPOCH - 1;
    let state = get_state_from_id(ID::Slot(target_slot), &db).await?;

    // Check if the epoch is finalized
    let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|err| {
        ApiError::InternalError(format!("Failed to get finalized checkpoint: {err:?}"))
    })?;
    let is_finalized = epoch_value <= finalized_checkpoint.epoch;

    // Calculate rewards for all participation flags
    let (source_rewards, source_penalties) = state
        .get_flag_index_deltas(TIMELY_SOURCE_FLAG_INDEX)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to calculate source rewards: {err:?}"))
        })?;

    let (target_rewards, target_penalties) = state
        .get_flag_index_deltas(TIMELY_TARGET_FLAG_INDEX)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to calculate target rewards: {err:?}"))
        })?;

    let (head_rewards, head_penalties) = state
        .get_flag_index_deltas(TIMELY_HEAD_FLAG_INDEX)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to calculate head rewards: {err:?}"))
        })?;

    let (_, inactivity_penalties) = state.get_inactivity_penalty_deltas().map_err(|err| {
        ApiError::InternalError(format!("Failed to calculate inactivity penalties: {err:?}"))
    })?;

    // Compute ideal rewards for all unique effective balances
    let ideal_rewards = compute_ideal_attestation_rewards(&state)?;

    // Group deltas for cleaner function call
    let deltas = AttestationDeltas {
        source_rewards,
        source_penalties,
        target_rewards,
        target_penalties,
        head_rewards,
        head_penalties,
        inactivity_penalties,
    };

    // Compute total rewards for all eligible validators (with optional filtering)
    let total_rewards = compute_total_attestation_rewards(&state, &deltas, validator_ids.as_ref())?;

    let response = AttestationRewardsResponse {
        execution_optimistic: false,
        finalized: is_finalized,
        data: AttestationRewardsData {
            ideal_rewards,
            total_rewards,
        },
    };

    Ok(HttpResponse::Ok().json(response))
}
