use alloy_rlp::{BufMut, Decodable, Encodable, bytes::Bytes};
use anyhow::anyhow;
use discv5::Enr;
use ssz::Encode;
use ssz_types::{
    BitVector,
    typenum::{U4, U64},
};
use tracing::{error, trace};

use crate::config::SYNC_COMMITTEE_SUBNET_COUNT;

pub const ATTESTATION_BITFIELD_ENR_KEY: &str = "attnets";
pub const SYNC_COMMITTEE_BITFIELD_ENR_KEY: &str = "syncnets";

/// Represents a subnet that a validator can participate in
///
/// There are two types of subnets:
/// 1. Attestation subnets - Used for validator attestations
/// 2. Sync committee subnets - Used for sync committee duties
///
/// # Examples
///
/// ```
/// use ream_discv5::subnet::Subnet;
///
/// // An attestation subnet with index 3
/// let attestation_subnet = Subnet::Attestation(3);
///
/// // A sync committee subnet with index 1
/// let sync_committee_subnet = Subnet::SyncCommittee(1);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum Subnet {
    Attestation(u8),
    SyncCommittee(u8),
}

/// Represents the attestation subnets a node is subscribed to
///
/// This directly wraps a BitVector<U64> for the attestation subnet bitfield
/// and handles encoding/decoding to raw bytes for ENR records.
#[derive(Clone, Debug, Default)]
pub struct AttestationSubnets(pub BitVector<U64>);

impl AttestationSubnets {
    /// Create a new empty attestation subnets bitfield
    pub fn new() -> Self {
        Self(BitVector::new())
    }

    /// Set a specific attestation subnet bit
    pub fn set(&mut self, index: usize, value: bool) -> anyhow::Result<()> {
        self.0
            .set(index, value)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

    /// Get a specific attestation subnet bit
    pub fn get(&self, index: usize) -> anyhow::Result<bool> {
        self.0
            .get(index)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }
}

impl From<BitVector<U64>> for AttestationSubnets {
    fn from(bits: BitVector<U64>) -> Self {
        Self(bits)
    }
}

impl Encodable for AttestationSubnets {
    fn encode(&self, out: &mut dyn BufMut) {
        // Create a Bytes wrapper around the raw bytes to ensure proper RLP encoding
        let bytes = Bytes::from(self.0.as_ssz_bytes());
        bytes.encode(out);
    }
}

impl Decodable for AttestationSubnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        // Decode as RLP Bytes first
        let bytes = Bytes::decode(buf)?;
        if bytes.len() < 8 {
            return Err(alloy_rlp::Error::Custom(Box::leak(
                "Insufficient bytes for AttestationSubnets decoding"
                    .to_string()
                    .into_boxed_str(),
            )));
        }

        // Convert to BitVector
        let bitvector = BitVector::<U64>::from_bytes(bytes.to_vec().into())
            .map(Some)
            .unwrap_or(None);

        match bitvector {
            Some(bits) => Ok(AttestationSubnets(bits)),
            None => Err(alloy_rlp::Error::Custom(Box::leak(
                "Failed to decode AttestationSubnets bitfield"
                    .to_string()
                    .into_boxed_str(),
            ))),
        }
    }
}

/// Represents the sync committee subnets a node is subscribed to
///
/// This directly wraps a BitVector<U4> for the sync committee subnet bitfield
/// and handles encoding/decoding to raw bytes for ENR records.
#[derive(Clone, Debug, Default)]
pub struct SyncCommitteeSubnets(pub BitVector<U4>);

impl SyncCommitteeSubnets {
    /// Create a new empty sync committee subnets bitfield
    pub fn new() -> Self {
        Self(BitVector::new())
    }

    /// Set a specific sync committee subnet bit
    pub fn set(&mut self, index: usize, value: bool) -> anyhow::Result<()> {
        self.0
            .set(index, value)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

    /// Get a specific sync committee subnet bit
    pub fn get(&self, index: usize) -> anyhow::Result<bool> {
        self.0
            .get(index)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }
}

impl From<BitVector<U4>> for SyncCommitteeSubnets {
    fn from(bits: BitVector<U4>) -> Self {
        Self(bits)
    }
}

impl Encodable for SyncCommitteeSubnets {
    fn encode(&self, out: &mut dyn BufMut) {
        // Create a Bytes wrapper around the raw bytes to ensure proper RLP encoding
        let bytes = Bytes::from(self.0.as_ssz_bytes());
        bytes.encode(out);
    }
}

impl Decodable for SyncCommitteeSubnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        // Decode as RLP Bytes first
        let bytes = Bytes::decode(buf)?;
        if bytes.is_empty() {
            return Err(alloy_rlp::Error::Custom(Box::leak(
                "Insufficient bytes for SyncCommitteeSubnets decoding"
                    .to_string()
                    .into_boxed_str(),
            )));
        }

        // Convert to BitVector
        let bitvector = BitVector::<U4>::from_bytes(bytes.to_vec().into())
            .map(Some)
            .unwrap_or(None);

        match bitvector {
            Some(bits) => Ok(SyncCommitteeSubnets(bits)),
            None => Err(alloy_rlp::Error::Custom(Box::leak(
                "Failed to decode SyncCommitteeSubnets bitfield"
                    .to_string()
                    .into_boxed_str(),
            ))),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Subnets {
    pub attestation_bits: Option<BitVector<U64>>,
    pub sync_committee_bits: Option<BitVector<U4>>,
}

impl Subnets {
    pub fn new() -> Self {
        Self {
            attestation_bits: Some(BitVector::new()),
            sync_committee_bits: Some(BitVector::new()),
        }
    }

    pub fn enable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) => {
                if let Some(bits) = &mut self.attestation_bits {
                    if id < 64 {
                        bits.set(id as usize, true)
                            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                    }
                }
                Ok(())
            }
            Subnet::SyncCommittee(id) => {
                if let Some(bits) = &mut self.sync_committee_bits {
                    if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                        bits.set(id as usize, true)
                            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                    }
                }
                Ok(())
            }
        }
    }

    pub fn disable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) => {
                if let Some(bits) = &mut self.attestation_bits {
                    if id < 64 {
                        bits.set(id as usize, false)
                            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                    }
                }
                Ok(())
            }
            Subnet::SyncCommittee(id) => {
                if let Some(bits) = &mut self.sync_committee_bits {
                    if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                        bits.set(id as usize, false)
                            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                    }
                }
                Ok(())
            }
        }
    }

    pub fn is_active(&self, subnet: Subnet) -> anyhow::Result<bool> {
        match subnet {
            Subnet::Attestation(id) => {
                if let Some(bits) = &self.attestation_bits {
                    if id < 64 {
                        return bits
                            .get(id as usize)
                            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"));
                    }
                }
                Ok(false)
            }
            Subnet::SyncCommittee(id) => {
                if let Some(bits) = &self.sync_committee_bits {
                    if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                        return bits
                            .get(id as usize)
                            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"));
                    }
                }
                Ok(false)
            }
        }
    }
}

impl Encodable for Subnets {
    fn encode(&self, out: &mut dyn BufMut) {
        if let Some(bits) = &self.attestation_bits {
            let bytes = Bytes::from(bits.as_ssz_bytes());
            bytes.encode(out);
        } else {
            let bytes = Bytes::new();
            bytes.encode(out);
        }
    }
}

impl Decodable for Subnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;

        if bytes.len() >= 8 {
            let attestation_bits = BitVector::<U64>::from_bytes(bytes.to_vec().into())
                .map(Some)
                .unwrap_or(None);

            Ok(Subnets {
                attestation_bits,
                sync_committee_bits: None,
            })
        } else if !bytes.is_empty() {
            let sync_committee_bits = BitVector::<U4>::from_bytes(bytes.to_vec().into())
                .map(Some)
                .unwrap_or(None);

            Ok(Subnets {
                attestation_bits: None,
                sync_committee_bits,
            })
        } else {
            Err(alloy_rlp::Error::Custom(Box::leak(
                "Insufficient bytes for Subnets decoding"
                    .to_string()
                    .into_boxed_str(),
            )))
        }
    }
}

impl Subnets {
    pub fn encode_sync_committee(&self, out: &mut dyn BufMut) {
        if let Some(bits) = &self.sync_committee_bits {
            let bytes = Bytes::from(bits.as_ssz_bytes());
            bytes.encode(out);
        } else {
            let bytes = Bytes::new();
            bytes.encode(out);
        }
    }

    /// Decode from sync committee field specifically
    pub fn decode_sync_committee(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        // Decode as RLP Bytes first
        let bytes = Bytes::decode(buf)?;
        if bytes.is_empty() {
            return Err(alloy_rlp::Error::Custom(Box::leak(
                "Insufficient bytes for SyncCommitteeSubnets decoding"
                    .to_string()
                    .into_boxed_str(),
            )));
        }

        let sync_committee_bits = BitVector::<U4>::from_bytes(bytes.to_vec().into())
            .map(Some)
            .unwrap_or(None);

        Ok(Subnets {
            attestation_bits: None,
            sync_committee_bits,
        })
    }
}

// Add enable_subnet and disable_subnet methods to AttestationSubnets
impl AttestationSubnets {
    /// Enable a subnet by setting its bit to true
    pub fn enable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) => {
                if id < 64 {
                    self.set(id as usize, true)?;
                }
                Ok(())
            }
            _ => Err(anyhow!(
                "Cannot enable non-attestation subnet in AttestationSubnets"
            )),
        }
    }

    /// Disable a subnet by setting its bit to false
    pub fn disable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) => {
                if id < 64 {
                    self.set(id as usize, false)?;
                }
                Ok(())
            }
            _ => Err(anyhow!(
                "Cannot disable non-attestation subnet in AttestationSubnets"
            )),
        }
    }
}

// Add enable_subnet and disable_subnet methods to SyncCommitteeSubnets
impl SyncCommitteeSubnets {
    /// Enable a subnet by setting its bit to true
    pub fn enable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::SyncCommittee(id) => {
                if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                    self.set(id as usize, true)?;
                }
                Ok(())
            }
            _ => Err(anyhow!(
                "Cannot enable non-sync committee subnet in SyncCommitteeSubnets"
            )),
        }
    }

    /// Disable a subnet by setting its bit to false
    pub fn disable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::SyncCommittee(id) => {
                if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                    self.set(id as usize, false)?;
                }
                Ok(())
            }
            _ => Err(anyhow!(
                "Cannot disable non-sync committee subnet in SyncCommitteeSubnets"
            )),
        }
    }
}

pub fn subnet_predicate(subnets: Vec<Subnet>) -> impl Fn(&Enr) -> bool + Send + Sync {
    move |enr: &Enr| {
        // Check if there are any sync committee subnets to match
        let has_sync_committee_subnets = subnets
            .iter()
            .any(|subnet| matches!(subnet, Subnet::SyncCommittee(_)));
        let has_attestation_subnets = subnets
            .iter()
            .any(|subnet| matches!(subnet, Subnet::Attestation(_)));

        // If we don't have any subnets to match, return
        if subnets.is_empty() {
            return true;
        }

        // Check for attestation subnets
        let attestation_match = if has_attestation_subnets {
            // Get the AttestationSubnets type
            let attestation_bits =
                match enr.get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY) {
                    Some(Ok(subnets)) => Some(subnets),
                    _ => {
                        trace!(
                            "Peer rejected: invalid or missing attnets field; peer_id: {}",
                            enr.node_id()
                        );
                        return false;
                    }
                };

            let Some(attestation_bits) = attestation_bits else {
                trace!(
                    "Peer rejected: invalid or missing attnets field; peer_id: {}",
                    enr.node_id()
                );
                return false;
            };

            let mut matches_subnet = false;
            for subnet in &subnets {
                if let Subnet::Attestation(id) = subnet {
                    if *id >= 64 {
                        error!(
                            "Peer rejected: subnet ID {} exceeds attestation bitfield length; peer_id: {}",
                            id,
                            enr.node_id()
                        );
                        return false;
                    }
                    matches_subnet |= match attestation_bits.get(*id as usize) {
                        Ok(true) => true,
                        Ok(false) => {
                            trace!(
                                "Peer found but not on attestation subnet {}; peer_id: {}",
                                id,
                                enr.node_id()
                            );
                            false
                        }
                        Err(err) => {
                            error!(
                                ?err,
                                "Peer rejected: invalid attestation bitfield index; subnet_id: {}, peer_id: {}",
                                id,
                                enr.node_id()
                            );
                            false
                        }
                    };
                }
            }
            matches_subnet
        } else {
            true
        };

        // Check for sync committee subnets
        let sync_committee_match = if has_sync_committee_subnets {
            let sync_committee_bits =
                match enr.get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY) {
                    Some(Ok(subnets)) => Some(subnets),
                    _ => {
                        trace!(
                            "Peer rejected: missing syncnets field; peer_id: {}",
                            enr.node_id()
                        );
                        return false;
                    }
                };

            let Some(sync_committee_bits) = sync_committee_bits else {
                trace!(
                    "Peer rejected: invalid or missing syncnets field; peer_id: {}",
                    enr.node_id()
                );
                return false;
            };

            let mut matches_subnet = false;
            for subnet in &subnets {
                if let Subnet::SyncCommittee(id) = subnet {
                    if *id >= SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                        trace!(
                            "Peer rejected: subnet ID {} exceeds sync committee bitfield length; peer_id: {}",
                            id,
                            enr.node_id()
                        );
                        return false;
                    }
                    matches_subnet |= match sync_committee_bits.get(*id as usize) {
                        Ok(true) => true,
                        Ok(false) => {
                            trace!(
                                "Peer found but not on sync committee subnet {}; peer_id: {}",
                                id,
                                enr.node_id()
                            );
                            false
                        }
                        Err(err) => {
                            trace!(
                                ?err,
                                "Peer rejected: invalid sync committee bitfield index; subnet_id: {}, peer_id: {}",
                                id,
                                enr.node_id()
                            );
                            false
                        }
                    };
                }
            }
            matches_subnet
        } else {
            true
        };

        // If there are both attestation and sync committee subnets to match,
        // require both to match. Otherwise, just require the one we're looking for.
        if has_attestation_subnets && has_sync_committee_subnets {
            attestation_match && sync_committee_match
        } else if has_attestation_subnets {
            attestation_match
        } else {
            sync_committee_match
        }
    }
}

/// Compute which sync committee subnet the validator should be on
///
/// Returns the subnet index for the validator's position in the sync committee
pub fn compute_subnet_for_sync_committee_index(sync_committee_index: usize) -> u8 {
    // Calculate which subnet this validator should be in
    (sync_committee_index % SYNC_COMMITTEE_SUBNET_COUNT) as u8
}

/// Compute the subnets for a validator's current sync committee assignment
///
/// Returns a vector of subnet indices that the validator should be subscribed to
pub fn compute_subnets_for_sync_committee(sync_committee_indices: &[usize]) -> Vec<u8> {
    sync_committee_indices
        .iter()
        .map(|&index| compute_subnet_for_sync_committee_index(index))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use discv5::{
        Enr,
        enr::{CombinedKey, k256::ecdsa::SigningKey},
    };

    use super::*;

    #[test]
    fn test_compute_subnet_for_sync_committee_index() {
        // Test with specific cases
        assert_eq!(compute_subnet_for_sync_committee_index(0), 0);
        assert_eq!(compute_subnet_for_sync_committee_index(1), 1);
        assert_eq!(compute_subnet_for_sync_committee_index(2), 2);
        assert_eq!(compute_subnet_for_sync_committee_index(3), 3);
        assert_eq!(compute_subnet_for_sync_committee_index(4), 0);
        assert_eq!(compute_subnet_for_sync_committee_index(5), 1);

        // Test with larger indices
        assert_eq!(compute_subnet_for_sync_committee_index(100), 0);
        assert_eq!(compute_subnet_for_sync_committee_index(101), 1);
        assert_eq!(compute_subnet_for_sync_committee_index(102), 2);
        assert_eq!(compute_subnet_for_sync_committee_index(103), 3);
    }

    #[test]
    fn test_compute_subnets_for_sync_committee() {
        // Test with specific cases
        let indices = vec![0, 4, 8, 12];
        assert_eq!(
            compute_subnets_for_sync_committee(&indices),
            vec![0, 0, 0, 0]
        );

        let indices = vec![1, 5, 9, 13];
        assert_eq!(
            compute_subnets_for_sync_committee(&indices),
            vec![1, 1, 1, 1]
        );

        let indices = vec![0, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(
            compute_subnets_for_sync_committee(&indices),
            vec![0, 1, 2, 3, 0, 1, 2, 3]
        );

        // Test with mixed indices
        let indices = vec![2, 6, 10, 14];
        assert_eq!(
            compute_subnets_for_sync_committee(&indices),
            vec![2, 2, 2, 2]
        );
    }

    #[test]
    fn test_subnets_sync_committee() {
        let mut subnets = Subnets::new();

        // Test enabling sync committee subnet
        assert!(!subnets.is_active(Subnet::SyncCommittee(0)).unwrap_or(false));
        subnets.enable_subnet(Subnet::SyncCommittee(0)).unwrap();
        assert!(subnets.is_active(Subnet::SyncCommittee(0)).unwrap_or(false));

        // Test enabling another sync committee subnet
        subnets.enable_subnet(Subnet::SyncCommittee(1)).unwrap();
        assert!(subnets.is_active(Subnet::SyncCommittee(1)).unwrap_or(false));

        // Test disabling sync committee subnet
        subnets.disable_subnet(Subnet::SyncCommittee(0)).unwrap();
        assert!(!subnets.is_active(Subnet::SyncCommittee(0)).unwrap_or(false));
        assert!(subnets.is_active(Subnet::SyncCommittee(1)).unwrap_or(false));

        // Test out-of-range sync committee subnet
        let result = subnets.enable_subnet(Subnet::SyncCommittee(10));
        assert!(result.is_ok()); // Should be handled gracefully
        assert!(
            !subnets
                .is_active(Subnet::SyncCommittee(10))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_subnets_attestation_and_sync_committee() {
        let mut subnets = Subnets::new();

        // Enable both attestation and sync committee subnets
        subnets.enable_subnet(Subnet::Attestation(5)).unwrap();
        subnets.enable_subnet(Subnet::SyncCommittee(2)).unwrap();

        // Verify both are active
        assert!(subnets.is_active(Subnet::Attestation(5)).unwrap_or(false));
        assert!(subnets.is_active(Subnet::SyncCommittee(2)).unwrap_or(false));

        // Disable attestation subnet
        subnets.disable_subnet(Subnet::Attestation(5)).unwrap();
        assert!(!subnets.is_active(Subnet::Attestation(5)).unwrap_or(false));
        assert!(subnets.is_active(Subnet::SyncCommittee(2)).unwrap_or(false));
    }

    #[test]
    fn test_decodes_subnets() {
        let enr = Enr::from_str("enr:-LS4QLe5eq5PFn1ZynqkrF6yg6ZGoplSDSNEPXtXfQh0vqhrDBQZICVoQu-AdeBOmtOFcAO7a0tJLdSlqStkdxkXnwaCCKSHYXR0bmV0c4gAAAAAAAAAMIRldGgykGqVoakEAAAA__________-CaWSCdjSCaXCEywwIqolzZWNwMjU2azGhA2JDBvnFqwtkUx34b_OdHXN1eO2JBMLWbzZXfGksk3YRg3RjcIIjkYN1ZHCCI5E").unwrap();

        let expected = BitVector::<U64>::from_bytes(vec![0, 0, 0, 0, 0, 0, 0, 48].into()).unwrap();
        let subnets = enr
            .get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .unwrap()
            .unwrap();

        // Compare the bits rather than direct struct comparison
        if let Some(actual) = &subnets.attestation_bits {
            for i in 0..64 {
                assert_eq!(actual.get(i).unwrap(), expected.get(i).unwrap());
            }
        } else {
            panic!("Expected attestation_bits to be Some, got None");
        }

        let enr = Enr::from_str("enr:-Ly4QHiJW24IzegmekAp3SRXhmopPLG-6PI7e-poXLDeaTcJC0yUtwg3XYELsw8v1-GkBByYpw6IaYDbtiaZLbwaOXUeh2F0dG5ldHOI__________-EZXRoMpBqlaGpBAAAAP__________gmlkgnY0gmlwhMb05QKJc2VjcDI1NmsxoQIMnwShvit2bpXbH0iPB3uyaPYTQ_dYOFl6TNp2h01zZohzeW5jbmV0cw-DdGNwgiMog3VkcIIjKA").unwrap();

        let expected = BitVector::<U64>::from_bytes(vec![255; 8].into()).unwrap();
        let subnets = enr
            .get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .unwrap()
            .unwrap();

        // Compare the bits rather than direct struct comparison
        if let Some(actual) = &subnets.attestation_bits {
            for i in 0..64 {
                assert_eq!(actual.get(i).unwrap(), expected.get(i).unwrap());
            }
        } else {
            panic!("Expected attestation_bits to be Some, got None");
        }
    }

    #[test]
    fn test_encode_decode_subnet_fields() {
        // Create ENR key
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        // Create and initialize subnets
        let mut subnets = Subnets::new();

        // Enable specific attestation subnets
        subnets.enable_subnet(Subnet::Attestation(1)).unwrap();
        subnets.enable_subnet(Subnet::Attestation(5)).unwrap();

        // Enable specific sync committee subnets
        subnets.enable_subnet(Subnet::SyncCommittee(0)).unwrap();
        subnets.enable_subnet(Subnet::SyncCommittee(2)).unwrap();

        // Create separate subnet objects for attestation and sync committee
        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.set(1, true).unwrap();
        attestation_subnets.set(5, true).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets.set(0, true).unwrap();
        sync_committee_subnets.set(2, true).unwrap();

        // Build ENR with both subnet fields
        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        // Decode attestation bitfield
        let decoded_attnets = enr
            .get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .expect("Failed to get attestation bitfield")
            .expect("Failed to decode attestation bitfield");

        // Verify attestation subnets
        assert!(decoded_attnets.is_active(Subnet::Attestation(1)).unwrap());
        assert!(decoded_attnets.is_active(Subnet::Attestation(5)).unwrap());
        assert!(!decoded_attnets.is_active(Subnet::Attestation(0)).unwrap());
        assert!(!decoded_attnets.is_active(Subnet::Attestation(10)).unwrap());

        // Decode sync committee bitfield
        let decoded_syncnets = enr
            .get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY)
            .expect("Failed to get sync committee bitfield")
            .expect("Failed to decode sync committee bitfield");

        // Convert to our Subnets type for verification
        let mut subnets_from_sync = Subnets::new();
        if let Some(bits) = &mut subnets_from_sync.sync_committee_bits {
            for i in 0..SYNC_COMMITTEE_SUBNET_COUNT {
                if decoded_syncnets.get(i).unwrap() {
                    bits.set(i, true).unwrap();
                }
            }
        }

        // Verify sync committee subnets
        assert!(
            subnets_from_sync
                .is_active(Subnet::SyncCommittee(0))
                .unwrap()
        );
        assert!(
            subnets_from_sync
                .is_active(Subnet::SyncCommittee(2))
                .unwrap()
        );
        assert!(
            !subnets_from_sync
                .is_active(Subnet::SyncCommittee(1))
                .unwrap()
        );
        assert!(
            !subnets_from_sync
                .is_active(Subnet::SyncCommittee(3))
                .unwrap()
        );
    }

    #[test]
    fn test_new_subnet_types() {
        // Create ENR key
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        // Create attestation subnets
        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.set(3, true).unwrap();
        attestation_subnets.set(42, true).unwrap();

        // Create sync committee subnets
        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets.set(0, true).unwrap();
        sync_committee_subnets.set(2, true).unwrap();

        // Build ENR with both subnet fields using the new types
        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        // Verify attestation subnets
        let decoded_attnets = enr
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .expect("Failed to get attestation subnets")
            .expect("Failed to decode attestation subnets");

        assert!(decoded_attnets.get(3).unwrap());
        assert!(decoded_attnets.get(42).unwrap());
        assert!(!decoded_attnets.get(0).unwrap());
        assert!(!decoded_attnets.get(10).unwrap());

        // Verify sync committee subnets
        let decoded_syncnets = enr
            .get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY)
            .expect("Failed to get sync committee subnets")
            .expect("Failed to decode sync committee subnets");

        assert!(decoded_syncnets.get(0).unwrap());
        assert!(decoded_syncnets.get(2).unwrap());
        assert!(!decoded_syncnets.get(1).unwrap());
        assert!(!decoded_syncnets.get(3).unwrap());

        // Test subnet predicate with the new types
        let attestation_subnet = Subnet::Attestation(3);
        let sync_committee_subnet = Subnet::SyncCommittee(2);

        // Should match both attestation and sync committee subnets
        let predicate = subnet_predicate(vec![
            attestation_subnet.clone(),
            sync_committee_subnet.clone(),
        ]);
        assert!(predicate(&enr));

        // Should match only attestation subnet
        let predicate = subnet_predicate(vec![attestation_subnet]);
        assert!(predicate(&enr));

        // Should match only sync committee subnet
        let predicate = subnet_predicate(vec![sync_committee_subnet]);
        assert!(predicate(&enr));

        // Should not match non-subscribed subnets
        let predicate = subnet_predicate(vec![Subnet::Attestation(10), Subnet::SyncCommittee(1)]);
        assert!(!predicate(&enr));
    }

    #[test]
    fn test_backward_compatibility() {
        // Create ENR key
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        // Create separate subnet types for attestation and sync committee
        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.set(5, true).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets.set(1, true).unwrap();

        // Build ENR with both subnet fields using the new types
        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        // Test subnet predicate with the new types (should still work with the predicate)
        let attestation_subnet = Subnet::Attestation(5);
        let sync_committee_subnet = Subnet::SyncCommittee(1);

        // Should match both attestation and sync committee subnets
        let predicate = subnet_predicate(vec![
            attestation_subnet.clone(),
            sync_committee_subnet.clone(),
        ]);
        assert!(predicate(&enr));

        // Should match only attestation subnet
        let predicate = subnet_predicate(vec![attestation_subnet]);
        assert!(predicate(&enr));

        // Should match only sync committee subnet
        let predicate = subnet_predicate(vec![sync_committee_subnet]);
        assert!(predicate(&enr));

        // Should not match non-subscribed subnets
        let predicate = subnet_predicate(vec![Subnet::Attestation(10), Subnet::SyncCommittee(2)]);
        assert!(!predicate(&enr));
    }
}
