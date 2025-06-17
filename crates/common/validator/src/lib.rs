use ethereum_hashing::hash;
use ream_bls::BLSSignature;

pub mod aggregate_and_proof;
pub mod attestation;
pub mod beacon_api_client;
pub mod blob_sidecars;
pub mod block;
pub mod constants;
pub mod contribution_and_proof;
pub mod execution_requests;
pub mod randao;
pub mod state;
pub mod sync_committee;
pub mod validator;

pub fn hash_signature_prefix_to_u64(signature: BLSSignature) -> u64 {
    let mut hash_prefix_bytes = [0u8; 8];
    hash_prefix_bytes.copy_from_slice(&hash(signature.to_slice())[..8]);
    u64::from_le_bytes(hash_prefix_bytes)
}
