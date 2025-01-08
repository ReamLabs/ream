use alloy_primitives::B256;
use tree_hash::TreeHash;

use crate::signing_data::SigningData;

pub fn compute_signing_root<SSZObject: TreeHash>(ssz_object: SSZObject, domain: B256) -> B256 {
    SigningData {
        object_root: ssz_object.tree_hash_root(),
        domain,
    }
    .tree_hash_root()
}
