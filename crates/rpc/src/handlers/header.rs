use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path, Query},
};
use alloy_primitives::B256;
use ream_beacon_api_types::{
    error::ApiError,
    id::ID,
    query::{ParentRootQuery, SlotQuery},
    responses::BeaconResponse,
};
use ream_consensus::beacon_block_header::SignedBeaconBlockHeader;
use ream_storage::{db::ReamDB, tables::Table};
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;

use super::block::get_beacon_block_from_id;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HeaderData {
    root: B256,
    canonical: bool,
    header: SignedBeaconBlockHeader,
}

impl HeaderData {
    pub fn new(root: B256, canonical: bool, header: SignedBeaconBlockHeader) -> Self {
        Self {
            root,
            canonical,
            header,
        }
    }
}

/// Called using `/eth/v1/beacon/headers`
/// Optional paramaters `slot` and/or `parent_root`
#[get("/beacon/headers")]
pub async fn get_headers(
    db: Data<ReamDB>,
    slot: Query<SlotQuery>,
    parent_root: Query<ParentRootQuery>,
) -> Result<impl Responder, ApiError> {
    let (header, root) = match (slot.slot, parent_root.parent_root) {
        (None, None) => get_header_from_slot(None, &db).await?,
        (None, Some(parent_root)) => {
            // get parent block to have access to `slot`
            let parent_block = db
                .beacon_block_provider()
                .get(parent_root)
                .map_err(|err| {
                    ApiError::InternalError(format!("Failed to get headers, error: {err:?}"))
                })?
                .ok_or_else(|| ApiError::NotFound(String::from("Unable to fetch parent block")))?;

            // fetch block header at `slot+1`
            let (child_header, child_block_root) =
                get_header_from_slot(Some(parent_block.message.slot + 1), &db).await?;

            if child_header.message.parent_root != parent_root {
                return Err(ApiError::NotFound(format!(
                    "Header with parent root :{parent_root:?}"
                )))?;
            }

            (child_header, child_block_root)
        }
        (Some(slot), None) => get_header_from_slot(Some(slot), &db).await?,
        (Some(slot), Some(parent_root)) => {
            let (header, root) = get_header_from_slot(Some(slot), &db).await?;
            if header.message.parent_root == parent_root {
                (header, root)
            } else {
                return Err(ApiError::NotFound(format!(
                    "Header at slot: {slot} with parent root: {parent_root:?} not found"
                )))?;
            }
        }
    };

    Ok(HttpResponse::Ok().json(BeaconResponse::new(HeaderData::new(root, true, header))))
}

/// Called using `/eth/v1/beacon/headers/{block_id}`
#[get("/beacon/headers/{block_id}")]
pub async fn get_headers_from_block(
    block_id: Path<ID>,
    db: Data<ReamDB>,
) -> Result<impl Responder, ApiError> {
    let block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;
    let header = block.signed_header();

    Ok(HttpResponse::Ok().json(BeaconResponse::new(HeaderData::new(
        header.message.tree_hash_root(),
        true,
        header,
    ))))
}

pub async fn get_header_from_slot(
    slot: Option<u64>,
    db: &ReamDB,
) -> Result<(SignedBeaconBlockHeader, B256), ApiError> {
    let slot = match slot {
        Some(slot) => slot,
        None => db
            .slot_index_provider()
            .get_highest_slot()
            .map_err(|err| {
                ApiError::InternalError(format!("Failed to get headers, error: {err:?}"))
            })?
            .ok_or_else(|| ApiError::NotFound(String::from("Unable to fetch latest slot")))?,
    };

    let beacon_block = get_beacon_block_from_id(ID::Slot(slot), db).await?;

    let header = beacon_block.signed_header();
    let root = header.tree_hash_root();

    Ok((header, root))
}
