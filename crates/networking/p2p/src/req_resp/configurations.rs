use ssz_types::typenum::{U4, U64};

/// The maximum allowed size of uncompressed payload in gossipsub messages and RPC chunks
pub const MAX_PAYLOAD_SIZE: u64 = 10485760;

/// The number of attestation subnets used in the gossipsub protocol.
pub type AttestationSubnetCount = U64;

/// The number of sync committee subnets used in the gossipsub aggregation protocol.
pub type SyncCommitteeSubnetCount = U4;
