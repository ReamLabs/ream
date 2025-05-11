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
        let bytes = Bytes::from(self.0.as_ssz_bytes());
        bytes.encode(out);
    }
}

impl Decodable for AttestationSubnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;
        let subnets = BitVector::<U64>::from_bytes(bytes.to_vec().into()).map_err(|err| {
            alloy_rlp::Error::Custom(Box::leak(
                format!("Failed to decode SSZ AttestationSubnets: {err:?}").into_boxed_str(),
            ))
        })?;
        Ok(Self(subnets))
    }
}

/// Represents the sync committee subnets a node is subscribed to
///
/// This directly wraps a BitVector<U4> for the sync committee subnet bitfield
/// and handles encoding/decoding to raw bytes for ENR records.
#[derive(Clone, Debug, Default)]
pub struct SyncCommitteeSubnets(pub BitVector<U4>);

impl SyncCommitteeSubnets {
    pub fn new() -> Self {
        Self(BitVector::new())
    }

    pub fn set(&mut self, index: usize, value: bool) -> anyhow::Result<()> {
        self.0
            .set(index, value)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

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
        let bytes = Bytes::from(self.0.as_ssz_bytes());
        bytes.encode(out);
    }
}

impl Decodable for SyncCommitteeSubnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;
        let subnets = BitVector::<U4>::from_bytes(bytes.to_vec().into()).map_err(|err| {
            alloy_rlp::Error::Custom(Box::leak(
                format!("Failed to decode SSZ SyncCommitteeSubnets: {err:?}").into_boxed_str(),
            ))
        })?;
        Ok(Self(subnets))
    }
}

impl AttestationSubnets {
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

impl SyncCommitteeSubnets {
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
        let has_sync_committee_subnets = subnets
            .iter()
            .any(|subnet| matches!(subnet, Subnet::SyncCommittee(_)));
        let has_attestation_subnets = subnets
            .iter()
            .any(|subnet| matches!(subnet, Subnet::Attestation(_)));

        if subnets.is_empty() {
            return true;
        }

        let attestation_match = if has_attestation_subnets {
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

        if has_attestation_subnets && has_sync_committee_subnets {
            attestation_match && sync_committee_match
        } else if has_attestation_subnets {
            attestation_match
        } else {
            sync_committee_match
        }
    }
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
    fn test_decodes_subnets() {
        let enr = Enr::from_str("enr:-LS4QLe5eq5PFn1ZynqkrF6yg6ZGoplSDSNEPXtXfQh0vqhrDBQZICVoQu-AdeBOmtOFcAO7a0tJLdSlqStkdxkXnwaCCKSHYXR0bmV0c4gAAAAAAAAAMIRldGgykGqVoakEAAAA__________-CaWSCdjSCaXCEywwIqolzZWNwMjU2azGhA2JDBvnFqwtkUx34b_OdHXN1eO2JBMLWbzZXfGksk3YRg3RjcIIjkYN1ZHCCI5E").unwrap();

        let expected = BitVector::<U64>::from_bytes(vec![0, 0, 0, 0, 0, 0, 0, 48].into()).unwrap();
        let attnets = enr
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .unwrap()
            .unwrap();

        for i in 0..64 {
            assert_eq!(attnets.get(i).unwrap(), expected.get(i).unwrap());
        }

        let enr = Enr::from_str("enr:-Ly4QHiJW24IzegmekAp3SRXhmopPLG-6PI7e-poXLDeaTcJC0yUtwg3XYELsw8v1-GkBByYpw6IaYDbtiaZLbwaOXUeh2F0dG5ldHOI__________-EZXRoMpBqlaGpBAAAAP__________gmlkgnY0gmlwhMb05QKJc2VjcDI1NmsxoQIMnwShvit2bpXbH0iPB3uyaPYTQ_dYOFl6TNp2h01zZohzeW5jbmV0cw-DdGNwgiMog3VkcIIjKA").unwrap();

        let expected = BitVector::<U64>::from_bytes(vec![255; 8].into()).unwrap();
        let attnets = enr
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .unwrap()
            .unwrap();

        for i in 0..64 {
            assert_eq!(attnets.get(i).unwrap(), expected.get(i).unwrap());
        }
    }

    #[test]
    fn test_encode_decode_subnet_fields() {
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.set(1, true).unwrap();
        attestation_subnets.set(5, true).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets.set(0, true).unwrap();
        sync_committee_subnets.set(2, true).unwrap();

        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        let decoded_attnets = enr
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .expect("Failed to get attestation bitfield")
            .expect("Failed to decode attestation bitfield");

        assert!(decoded_attnets.get(1).unwrap());
        assert!(decoded_attnets.get(5).unwrap());
        assert!(!decoded_attnets.get(0).unwrap());
        assert!(!decoded_attnets.get(10).unwrap());

        let decoded_syncnets = enr
            .get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY)
            .expect("Failed to get sync committee bitfield")
            .expect("Failed to decode sync committee bitfield");

        assert!(decoded_syncnets.get(0).unwrap());
        assert!(decoded_syncnets.get(2).unwrap());
        assert!(!decoded_syncnets.get(1).unwrap());
        assert!(!decoded_syncnets.get(3).unwrap());
    }

    #[test]
    fn test_new_subnet_types() {
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.set(3, true).unwrap();
        attestation_subnets.set(42, true).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets.set(0, true).unwrap();
        sync_committee_subnets.set(2, true).unwrap();

        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        let decoded_attnets = enr
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .expect("Failed to get attestation subnets")
            .expect("Failed to decode attestation subnets");

        assert!(decoded_attnets.get(3).unwrap());
        assert!(decoded_attnets.get(42).unwrap());
        assert!(!decoded_attnets.get(0).unwrap());
        assert!(!decoded_attnets.get(10).unwrap());

        let decoded_syncnets = enr
            .get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY)
            .expect("Failed to get sync committee subnets")
            .expect("Failed to decode sync committee subnets");

        assert!(decoded_syncnets.get(0).unwrap());
        assert!(decoded_syncnets.get(2).unwrap());
        assert!(!decoded_syncnets.get(1).unwrap());
        assert!(!decoded_syncnets.get(3).unwrap());

        let attestation_subnet = Subnet::Attestation(3);
        let sync_committee_subnet = Subnet::SyncCommittee(2);

        let predicate = subnet_predicate(vec![
            attestation_subnet.clone(),
            sync_committee_subnet.clone(),
        ]);
        assert!(predicate(&enr));

        let predicate = subnet_predicate(vec![attestation_subnet]);
        assert!(predicate(&enr));

        let predicate = subnet_predicate(vec![sync_committee_subnet]);
        assert!(predicate(&enr));

        let predicate = subnet_predicate(vec![Subnet::Attestation(10), Subnet::SyncCommittee(1)]);
        assert!(!predicate(&enr));
    }

    #[test]
    fn test_backward_compatibility() {
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.set(5, true).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets.set(1, true).unwrap();

        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        let attestation_subnet = Subnet::Attestation(5);
        let sync_committee_subnet = Subnet::SyncCommittee(1);

        let predicate = subnet_predicate(vec![
            attestation_subnet.clone(),
            sync_committee_subnet.clone(),
        ]);
        assert!(predicate(&enr));

        let predicate = subnet_predicate(vec![attestation_subnet]);
        assert!(predicate(&enr));

        let predicate = subnet_predicate(vec![sync_committee_subnet]);
        assert!(predicate(&enr));

        let predicate = subnet_predicate(vec![Subnet::Attestation(10), Subnet::SyncCommittee(2)]);
        assert!(!predicate(&enr));
    }
}
