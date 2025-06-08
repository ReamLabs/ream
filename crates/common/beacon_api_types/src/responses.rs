use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

pub const VERSION: &str = "electra";
pub const ETH_CONSENSUS_VERSION_HEADER: &str = "Eth-Consensus-Version";
const EXECUTION_OPTIMISTIC: bool = false;
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
