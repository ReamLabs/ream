use crate::constants::BLOB_SIDECAR_SUBNET_COUNT_ELECTRA;

pub fn compute_subnet_for_blob_sidecar(blob_index: u64) -> u64 {
    blob_index % BLOB_SIDECAR_SUBNET_COUNT_ELECTRA
}
