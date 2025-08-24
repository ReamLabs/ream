use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::BitVector;

use crate::req_resp::configurations::{AttestationSubnetCount, SyncCommitteeSubnetCount};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct GetMetaDataV2 {
    pub seq_number: u64,
    pub attnets: BitVector<AttestationSubnetCount>,
    pub syncnets: BitVector<SyncCommitteeSubnetCount>,
}
