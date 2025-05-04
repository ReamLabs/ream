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

pub type AttestationBitfield = BitVector<U64>;
pub type SyncCommitteeBitfield = BitVector<U4>;

#[derive(Clone, Debug, Default)]
pub struct Subnets {
    attestation_bits: Option<BitVector<U64>>,
    sync_committee_bits: Option<BitVector<U4>>,
}

impl Subnets {
    pub fn new() -> Self {
        Self {
            attestation_bits: None,
            sync_committee_bits: None,
        }
    }

    pub fn enable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) if id < 64 => {
                let bits = self.attestation_bits.get_or_insert(BitVector::new());
                bits.set(id as usize, true)
                    .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                Ok(())
            }
            Subnet::Attestation(_) => Ok(()),
            Subnet::SyncCommittee(id) if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 => {
                let bits = self.sync_committee_bits.get_or_insert(BitVector::new());
                bits.set(id as usize, true)
                    .map_err(|err| anyhow!("Sync committee subnet ID out of bounds: {err:?}"))?;
                Ok(())
            }
            Subnet::SyncCommittee(_) => {
                trace!("Ignoring out-of-range sync committee subnet ID");
                Ok(())
            }
        }
    }

    pub fn disable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) if id < 64 => {
                if let Some(bits) = &mut self.attestation_bits {
                    bits.set(id as usize, false)
                        .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                }
                Ok(())
            }
            Subnet::Attestation(_) => Ok(()),
            Subnet::SyncCommittee(id) if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 => {
                if let Some(bits) = &mut self.sync_committee_bits {
                    bits.set(id as usize, false).map_err(|err| {
                        anyhow!("Sync committee subnet ID out of bounds: {err:?}")
                    })?;
                }
                Ok(())
            }
            Subnet::SyncCommittee(_) => {
                trace!("Ignoring out-of-range sync committee subnet ID");
                Ok(())
            }
        }
    }

    pub fn is_active(&self, subnet: Subnet) -> anyhow::Result<bool> {
        let active = match subnet {
            Subnet::Attestation(id) if id < 64 => {
                if let Some(ref attestation_bits) = self.attestation_bits {
                    attestation_bits
                        .get(id as usize)
                        .map_err(|err| anyhow!("Couldn't get expected attestation {:?}", err))?
                } else {
                    false
                }
            }
            Subnet::Attestation(_) => false,
            Subnet::SyncCommittee(id) if id < SYNC_COMMITTEE_SUBNET_COUNT as u8 => self
                .sync_committee_bits
                .as_ref()
                .is_some_and(|bits| bits.get(id as usize).unwrap_or(false)),
            Subnet::SyncCommittee(_) => false,
        };
        Ok(active)
    }
}

impl Encodable for Subnets {
    fn encode(&self, out: &mut dyn BufMut) {
        // Convert the struct to an SSZ representation
        let mut bytes = Vec::new();

        // Encode attestation_bits
        if let Some(attestation_bits) = &self.attestation_bits {
            let attestation_bytes = attestation_bits.as_ssz_bytes();
            bytes.extend_from_slice(&attestation_bytes);
        } else {
            // Encode an empty bitvector
            let empty: BitVector<U64> = BitVector::new();
            bytes.extend_from_slice(&empty.as_ssz_bytes());
        }

        // Encode sync_committee_bits
        if let Some(sync_committee_bits) = &self.sync_committee_bits {
            let sync_committee_bytes = sync_committee_bits.as_ssz_bytes();
            bytes.extend_from_slice(&sync_committee_bytes);
        } else {
            // Encode an empty bitvector
            let empty: BitVector<U4> = BitVector::new();
            bytes.extend_from_slice(&empty.as_ssz_bytes());
        }

        // Wrap in Bytes and encode
        let bytes = Bytes::from(bytes);
        bytes.encode(out);
    }
}

impl Decodable for Subnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;

        // Here we'd need to parse the bytes back into the struct
        // For simplicity, assuming a fixed format:
        // first 8 bytes for attestation_bits, next bytes for sync_committee_bits
        if bytes.len() < 8 {
            return Err(alloy_rlp::Error::Custom(Box::leak(
                "Insufficient bytes for Subnets decoding"
                    .to_string()
                    .into_boxed_str(),
            )));
        }

        let attestation_bytes = &bytes[0..8];
        let attestation_bits = BitVector::<U64>::from_bytes(attestation_bytes.to_vec().into()).ok();

        let sync_committee_bits = if bytes.len() > 8 {
            let sync_committee_bytes = &bytes[8..];
            BitVector::<U4>::from_bytes(sync_committee_bytes.to_vec().into()).ok()
        } else {
            None
        };

        Ok(Subnets {
            attestation_bits,
            sync_committee_bits,
        })
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
            let Some(Ok(subnets_state)) =
                enr.get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
            else {
                trace!(
                    "Peer rejected: invalid or missing attnets field; peer_id: {}",
                    enr.node_id()
                );
                return false;
            };
            let Some(attestation_bits) = &subnets_state.attestation_bits else {
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
            let Some(Ok(subnets_state)) =
                enr.get_decodable::<Subnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY)
            else {
                trace!(
                    "Peer rejected: missing syncnets field; peer_id: {}",
                    enr.node_id()
                );
                return false;
            };
            let Some(sync_committee_bits) = &subnets_state.sync_committee_bits else {
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

        // Build ENR with both subnet fields
        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &subnets)
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
            .get_decodable::<Subnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY)
            .expect("Failed to get sync committee bitfield")
            .expect("Failed to decode sync committee bitfield");

        // Verify sync committee subnets
        assert!(
            decoded_syncnets
                .is_active(Subnet::SyncCommittee(0))
                .unwrap()
        );
        assert!(
            decoded_syncnets
                .is_active(Subnet::SyncCommittee(2))
                .unwrap()
        );
        assert!(
            !decoded_syncnets
                .is_active(Subnet::SyncCommittee(1))
                .unwrap()
        );
        assert!(
            !decoded_syncnets
                .is_active(Subnet::SyncCommittee(3))
                .unwrap()
        );
    }
}
