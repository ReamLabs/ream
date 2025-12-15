use ream_bls::BLSSignature;
use ream_consensus_misc::attestation_data::AttestationData;
use serde::{Deserialize, Serialize};
use ssz_types::{
    BitList, BitVector,
    typenum::{U64, U131072},
};

/// Attestation event.
///
/// The node has received an Attestation (from P2P or API) that passes validation
/// rules of the `beacon_attestation_{subnet_id}` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationEvent {
    pub aggregation_bits: BitList<U131072>,
    pub data: AttestationData,
    pub signature: BLSSignature,
    pub committee_bits: BitVector<U64>,
}

/// Single attestation event.
///
/// The node has received a SingleAttestation (from P2P or API) that passes validation
/// rules of the beacon_attestation_{subnet_id} topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleAttestationEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub committee_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub attester_index: u64,
    pub data: AttestationData,
    pub signature: BLSSignature,
}
