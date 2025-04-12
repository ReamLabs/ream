use alloy_primitives::B256;
use ream_consensus::beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader};
use ream_storage::{db::ReamDB, tables::Table};
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::block::get_beacon_block_from_id;
use crate::types::{
    errors::ApiError,
    id::ID,
    query::{ParentRootQuery, SlotQuery},
    response::BeaconResponse,
};

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
pub async fn get_headers(
    slot: SlotQuery,
    parent_root: ParentRootQuery,
    db: ReamDB,
) -> Result<impl Reply, Rejection> {
    let (header, root) = match (slot.slot, parent_root.parent_root) {
        (None, None) => {
            let slot = db
                .slot_index_provider()
                .get_highest_slot()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| ApiError::NotFound(String::from("Unable to fetch latest slot")))?;

            get_header_from_slot(slot, db).await?
        }
        (None, Some(parent_root)) => {
            // get parent block to have access to `slot`
            let parent_block = db
                .beacon_block_provider()
                .get(parent_root)
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| ApiError::NotFound(String::from("Unable to fetch parent block")))?;

            // fetch block header at `slot+1`
            let (child_header, child_block_root) =
                get_header_from_slot(parent_block.message.slot + 1, db)
                    .await
                    .map_err(|_| {
                        ApiError::NotFound(format!(
                            "Unable to fetch header with parent root: {parent_root:?}"
                        ))
                    })?;

            if child_header.message.parent_root != parent_root {
                return Err(ApiError::NotFound(format!(
                    "Header with parent root :{parent_root:?}"
                )))?;
            }

            (child_header, child_block_root)
        }
        (Some(slot), None) => get_header_from_slot(slot, db).await?,
        (Some(slot), Some(parent_root)) => {
            let (header, root) = get_header_from_slot(slot, db).await?;
            if header.message.parent_root == parent_root {
                (header, root)
            } else {
                return Err(ApiError::NotFound(format!(
                    "Header at slot: {slot} with parent root: {parent_root:?} not found"
                )))?;
            }
        }
    };

    Ok(with_status(
        BeaconResponse::json(HeaderData::new(root, true, header)),
        StatusCode::OK,
    ))
}

pub async fn get_header_from_slot(
    slot: u64,
    db: ReamDB,
) -> Result<(SignedBeaconBlockHeader, B256), ApiError> {
    let beacon_block = get_beacon_block_from_id(ID::Slot(slot), &db).await?;

    let header_message = BeaconBlockHeader {
        slot: beacon_block.message.slot,
        proposer_index: beacon_block.message.proposer_index,
        state_root: beacon_block.message.state_root,
        parent_root: beacon_block.message.parent_root,
        body_root: beacon_block.message.body.tree_hash_root(),
    };
    let root = header_message.tree_hash_root();

    Ok((
        SignedBeaconBlockHeader {
            message: header_message,
            signature: beacon_block.signature,
        },
        root,
    ))
}
