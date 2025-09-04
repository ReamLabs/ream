use std::vec;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path, Query},
};
use ream_api_types_beacon::{
    query::{EpochQuery, IndexQuery, SlotQuery},
    responses::BeaconResponse,
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_consensus_misc::{constants::beacon::SLOTS_PER_EPOCH, misc::compute_start_slot_at_epoch};
use ream_storage::db::beacon::BeaconDB;
use serde::Serialize;

use super::state::get_state_from_id;

#[derive(Debug, Serialize, Clone)]
pub struct CommitteeData {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    #[serde(with = "serde_utils::quoted_u64_vec")]
    pub validators: Vec<u64>,
}

impl CommitteeData {
    pub fn new(index: u64, slot: u64, validators: Vec<u64>) -> Self {
        Self {
            index,
            slot,
            validators,
        }
    }
}

/// Called by `/states/<state_id>/committees` to get the Committee Data of state.
/// Optional `epoch`, `index` or `slot` can be provided.
#[get("/beacon/states/{state_id}/committees")]
pub async fn get_committees(
    state_id: Path<ID>,
    epoch: Query<EpochQuery>,
    index: Query<IndexQuery>,
    slot: Query<SlotQuery>,
    db: Data<BeaconDB>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    let epoch = epoch.epoch.unwrap_or(state.get_current_epoch());
    let committees_per_slot = state.get_committee_count_per_slot(epoch);

    let slots: Vec<u64> = match slot.slot {
        Some(slot) => vec![slot],
        None => {
            let start_slot = compute_start_slot_at_epoch(epoch);
            (start_slot..(start_slot + SLOTS_PER_EPOCH)).collect()
        }
    };

    let indices: Vec<u64> = match index.index {
        Some(index) => vec![index],
        None => (0..(committees_per_slot * SLOTS_PER_EPOCH)).collect(),
    };

    let mut result: Vec<CommitteeData> = Vec::with_capacity(slots.len() * indices.len());

    for slot in &slots {
        for index in &indices {
            let committee = state.get_beacon_committee(*slot, *index).map_err(|err| {
                ApiError::NotFound(format!(
                    "Committee with slot: {slot} and index: {index} not found {err:?}"
                ))
            })?;
            result.push(CommitteeData {
                index: *index,
                slot: *slot,
                validators: committee,
            });
        }
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(result)))
}
