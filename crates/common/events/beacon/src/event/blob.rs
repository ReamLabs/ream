use alloy_primitives::B256;
use ream_consensus_misc::polynomial_commitments::kzg_commitment::KZGCommitment;
use serde::{Deserialize, Serialize};

/// Blob sidecar event.
///
/// The node has received a BlobSidecar (from P2P or API) that passes all gossip
/// validations on the `blob_sidecar_{subnet_id}` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobSidecarEvent {
    pub block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub kzg_commitment: KZGCommitment,
    pub versioned_hash: B256,
}

/// Data column sidecar event.
///
/// The node has received a DataColumnSidecar (from P2P or API) that passes all gossip
/// validations on the `data_column_sidecar_{subnet_id}` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataColumnSidecarEvent {
    pub block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub kzg_commitments: Vec<KZGCommitment>,
}
