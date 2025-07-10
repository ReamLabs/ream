use alloy_primitives::B256;
use ream_consensus::checkpoint::Checkpoint;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

pub const ACCEPT_PRIORITY: &str = "application/octet-stream;q=1.0,application/json;q=0.9";
pub const ETH_CONSENSUS_VERSION_HEADER: &str = "Eth-Consensus-Version";
pub const EXECUTION_OPTIMISTIC: bool = false;
pub const JSON_ACCEPT_PRIORITY: &str = "application/json;q=1";
pub const JSON_CONTENT_TYPE: &str = "application/json";
pub const SSZ_CONTENT_TYPE: &str = "application/octet-stream";
pub const VERSION: &str = "electra";
const FINALIZED: bool = false;

/// A DataResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!(T)
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct DataResponse<T> {
    pub data: T,
}

impl<T: Serialize> DataResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

#[derive(Serialize, Deserialize)]
pub struct RootResponse {
    pub root: B256,
}

impl RootResponse {
    pub fn new(root: B256) -> Self {
        Self { root }
    }
}

/// A BeaconResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "execution_optimistic" : bool,
///     "finalized" : bool,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct BeaconResponse<T> {
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: T,
}

impl<T: Serialize> BeaconResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            finalized: FINALIZED,
        }
    }
}

/// A BeaconVersionedResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "version": "electra"
///     "execution_optimistic" : bool,
///     "finalized" : bool,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct BeaconVersionedResponse<T> {
    pub version: String,
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: T,
}

impl<T: Serialize> BeaconVersionedResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            version: VERSION.into(),
            data,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            finalized: FINALIZED,
        }
    }
}

/// A DataVersionedResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///     "version": "electra",
///     "data": T
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct DataVersionedResponse<T> {
    pub version: String,
    pub data: T,
}

impl<T: Serialize> DataVersionedResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            version: VERSION.into(),
            data,
        }
    }
}

/// A DutiesResponse data struct that can be used to wrap duty data
/// used for json rpc responses
///
/// # Example
/// {
///     "dependent_root": "0x...",
///     "execution_optimistic": false,
///     "data": [T]
/// }
#[derive(Debug, Deserialize, Serialize, Encode, Decode)]
pub struct DutiesResponse<T: Encode + Decode> {
    pub dependent_root: B256,
    pub execution_optimistic: bool,
    pub data: Vec<T>,
}

impl<T: Serialize + Encode + Decode> DutiesResponse<T> {
    pub fn new(dependent_root: B256, data: Vec<T>) -> Self {
        Self {
            dependent_root,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            data,
        }
    }
}

/// A SyncCommitteeDutiesResponse data struct that can be used to wrap duty data
/// for sync committee duties
/// used for json rpc responses
///
/// # Example
/// {
///     "execution_optimistic": false,
///     "data": [T]
/// }
#[derive(Debug, Deserialize, Serialize, Encode, Decode)]
pub struct SyncCommitteeDutiesResponse<T: Encode + Decode> {
    pub execution_optimistic: bool,
    pub data: Vec<T>,
}

impl<T: Serialize + Encode + Decode> SyncCommitteeDutiesResponse<T> {
    pub fn new(data: Vec<T>) -> Self {
        Self {
            execution_optimistic: EXECUTION_OPTIMISTIC,
            data,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BeaconHeadResponse {
    pub root: B256,
    pub slot: u64,
    pub execution_optimistic: bool,
}

impl BeaconHeadResponse {
    pub fn new(root: B256, slot: u64) -> Self {
        Self {
            root,
            slot,
            execution_optimistic: EXECUTION_OPTIMISTIC,
        }
    }
}

/// A ForkChoiceResponse data struct that is used for /debug/fork_choice endpoint.
///
/// # Example
///
/// ```json
/// {
///   "justified_checkpoint": {
///     "epoch": "1",
///     "root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
///   },
///   "finalized_checkpoint": {
///     "epoch": "1",
///     "root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
///   },
///   "fork_choice_nodes": [
///     {
///       "slot": "1",
///       "block_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
///       "parent_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
///       "justified_epoch": "1",
///       "finalized_epoch": "1",
///       "weight": "1",
///       "validity": "valid",
///       "execution_block_hash": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
///       "extra_data": {}
///     }
///   ],
///   "extra_data": {}
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct ForkChoiceResponse {
    pub justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,
    pub fork_choice_nodes: Vec<ForkChoiceNode>,
    pub extra_data: serde_json::Value,
}

impl ForkChoiceResponse {
    pub fn new(
        justified_checkpoint: Checkpoint,
        finalized_checkpoint: Checkpoint,
        fork_choice_nodes: Vec<ForkChoiceNode>,
    ) -> Self {
        Self {
            justified_checkpoint,
            finalized_checkpoint,
            fork_choice_nodes,
            extra_data: json!({}),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForkChoiceNode {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub block_root: B256,
    pub parent_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub justified_epoch: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub finalized_epoch: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub weight: u64,
    pub validity: ForkChoiceValidity,
    pub execution_block_hash: B256,
    pub extra_data: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ForkChoiceValidity {
    Valid,
    Invalid,
    Optimistic,
}
