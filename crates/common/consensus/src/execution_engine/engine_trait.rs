use alloy_primitives::B256;
use async_trait::async_trait;

use super::{new_payload_request::NewPayloadRequest, rpc_types::get_blobs::BlobsAndProofV1};

#[async_trait]
pub trait ExecutionApi {
    /// Return ``True`` if and only if ``new_payload_request`` is valid with respect to
    /// ``self.execution_state``.
    async fn verify_and_notify_new_payload(
        &self,
        new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<bool>;

    async fn engine_get_blobs_v1(
        &self,
        blob_version_hashes: Vec<B256>,
    ) -> anyhow::Result<Vec<Option<BlobsAndProofV1>>>;
}
