use alloy_rlp::{BufMut, Decodable, Encodable, bytes::Bytes};
use anyhow::{anyhow, ensure};
use discv5::Enr;
use ssz::Encode;
use ssz_types::{
    BitVector,
    typenum::{U4, U64},
};
use tracing::{error, trace};

pub const ATTESTATION_BITFIELD_ENR_KEY: &str = "attnets";
pub const ATTESTATION_SUBNET_COUNT: usize = 64;
pub const SYNC_COMMITTEE_BITFIELD_ENR_KEY: &str = "syncnets";
pub const SYNC_COMMITTEE_SUBNET_COUNT: usize = 4;

/// Represents the attestation subnets a node is subscribed to
///
/// This directly wraps a BitVector<U64> for the attestation subnet bitfield
/// and handles encoding/decoding to raw bytes for ENR records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttestationSubnets(pub BitVector<U64>);

impl AttestationSubnets {
    pub fn new() -> Self {
        Self(BitVector::new())
    }

    pub fn enable_attestation_subnet(&mut self, subnet_id: u8) -> anyhow::Result<()> {
        ensure!(
            subnet_id < ATTESTATION_SUBNET_COUNT as u8,
            "Subnet ID {subnet_id} exceeds maximum attestation subnet count {ATTESTATION_SUBNET_COUNT}",
        );
        self.0
            .set(subnet_id as usize, true)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

    pub fn disable_attestation_subnet(&mut self, subnet_id: u8) -> anyhow::Result<()> {
        ensure!(
            subnet_id < ATTESTATION_SUBNET_COUNT as u8,
            "Subnet ID {subnet_id} exceeds maximum attestation subnet count {ATTESTATION_SUBNET_COUNT}",
        );
        self.0
            .set(subnet_id as usize, false)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

    pub fn is_attestation_subnet_enabled(&self, subnet_id: u8) -> anyhow::Result<bool> {
        ensure!(
            subnet_id < ATTESTATION_SUBNET_COUNT as u8,
            "Subnet ID {subnet_id} exceeds maximum attestation subnet count {ATTESTATION_SUBNET_COUNT}",
        );
        self.0
            .get(subnet_id as usize)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncCommitteeSubnets(pub BitVector<U4>);

impl SyncCommitteeSubnets {
    pub fn new() -> Self {
        Self(BitVector::new())
    }

    pub fn enable_sync_committee_subnet(&mut self, subnet_id: u8) -> anyhow::Result<()> {
        ensure!(
            subnet_id < SYNC_COMMITTEE_SUBNET_COUNT as u8,
            "Subnet ID {subnet_id} exceeds maximum sync committee subnet count {SYNC_COMMITTEE_SUBNET_COUNT}",
        );
        self.0
            .set(subnet_id as usize, true)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

    pub fn disable_sync_committee_subnet(&mut self, subnet_id: u8) -> anyhow::Result<()> {
        ensure!(
            subnet_id < SYNC_COMMITTEE_SUBNET_COUNT as u8,
            "Subnet ID {subnet_id} exceeds maximum sync committee subnet count {SYNC_COMMITTEE_SUBNET_COUNT}",
        );
        self.0
            .set(subnet_id as usize, false)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
    }

    pub fn is_sync_committee_subnet_enabled(&self, subnet_id: u8) -> anyhow::Result<bool> {
        ensure!(
            subnet_id < SYNC_COMMITTEE_SUBNET_COUNT as u8,
            "Subnet ID {subnet_id} exceeds maximum sync committee subnet count {SYNC_COMMITTEE_SUBNET_COUNT}",
        );
        self.0
            .get(subnet_id as usize)
            .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))
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

pub fn attestation_subnet_predicate(subnets: Vec<u8>) -> impl Fn(&Enr) -> bool + Send + Sync {
    move |enr: &Enr| {
        if subnets.is_empty() {
            return true;
        }

        let attestation_bits =
            match enr.get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY) {
                Some(Ok(attestation_bits)) => attestation_bits,
                _ => {
                    trace!(
                        "Peer rejected: invalid or missing attnets field; peer_id: {}",
                        enr.node_id()
                    );
                    return false;
                }
            };

        for subnet_id in &subnets {
            if *subnet_id >= ATTESTATION_SUBNET_COUNT as u8 {
                error!(
                    "Peer rejected: subnet ID {} exceeds attestation bitfield length; peer_id: {}",
                    subnet_id,
                    enr.node_id()
                );
                return false;
            }

            if let Ok(true) = attestation_bits.is_attestation_subnet_enabled(*subnet_id) {
                return true;
            } else {
                trace!(
                    "Peer found but not on attestation subnet {}; peer_id: {}",
                    subnet_id,
                    enr.node_id()
                );
            }
        }

        false
    }
}

pub fn sync_committee_subnet_predicate(subnets: Vec<u8>) -> impl Fn(&Enr) -> bool + Send + Sync {
    move |enr: &Enr| {
        if subnets.is_empty() {
            return true;
        }

        let sync_committee_bits =
            match enr.get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY) {
                Some(Ok(sync_committee_bits)) => sync_committee_bits,
                _ => {
                    trace!(
                        "Peer rejected: missing syncnets field; peer_id: {}",
                        enr.node_id()
                    );
                    return false;
                }
            };

        for subnet_id in &subnets {
            if *subnet_id >= SYNC_COMMITTEE_SUBNET_COUNT as u8 {
                trace!(
                    "Peer rejected: subnet ID {} exceeds sync committee bitfield length; peer_id: {}",
                    subnet_id,
                    enr.node_id()
                );
                return false;
            }

            if let Ok(true) = sync_committee_bits.is_sync_committee_subnet_enabled(*subnet_id) {
                return true;
            } else {
                trace!(
                    "Peer found but not on sync committee subnet {}; peer_id: {}",
                    subnet_id,
                    enr.node_id()
                );
            }
        }

        false
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

        let expected_attestation_subnets = AttestationSubnets(
            BitVector::<U64>::from_bytes(vec![0, 0, 0, 0, 0, 0, 0, 48].into()).unwrap(),
        );

        let attestation_result =
            enr.get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY);
        assert!(
            attestation_result.is_some(),
            "Missing attestation subnet field in ENR"
        );
        let attestation_decode_result = attestation_result.unwrap();
        assert!(
            attestation_decode_result.is_ok(),
            "Failed to decode attestation subnet field"
        );
        let attestation_subnets = attestation_decode_result.unwrap();

        assert_eq!(attestation_subnets, expected_attestation_subnets);

        let enr = Enr::from_str("enr:-Ly4QHiJW24IzegmekAp3SRXhmopPLG-6PI7e-poXLDeaTcJC0yUtwg3XYELsw8v1-GkBByYpw6IaYDbtiaZLbwaOXUeh2F0dG5ldHOI__________-EZXRoMpBqlaGpBAAAAP__________gmlkgnY0gmlwhMb05QKJc2VjcDI1NmsxoQIMnwShvit2bpXbH0iPB3uyaPYTQ_dYOFl6TNp2h01zZohzeW5jbmV0cw-DdGNwgiMog3VkcIIjKA").unwrap();

        let expected_attestation_subnets =
            AttestationSubnets(BitVector::<U64>::from_bytes(vec![255; 8].into()).unwrap());

        let attestation_result =
            enr.get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY);
        assert!(
            attestation_result.is_some(),
            "Missing attestation subnet field in ENR"
        );
        let attestation_decode_result = attestation_result.unwrap();
        assert!(
            attestation_decode_result.is_ok(),
            "Failed to decode attestation subnet field"
        );
        let attestation_subnets = attestation_decode_result.unwrap();

        assert_eq!(attestation_subnets, expected_attestation_subnets);
    }

    #[test]
    fn test_encode_decode_subnet_fields() {
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.enable_attestation_subnet(1).unwrap();
        attestation_subnets.enable_attestation_subnet(5).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets
            .enable_sync_committee_subnet(0)
            .unwrap();
        sync_committee_subnets
            .enable_sync_committee_subnet(2)
            .unwrap();

        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        let attestation_result =
            enr.get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY);
        assert!(
            attestation_result.is_some(),
            "Missing attestation subnet field in ENR"
        );
        let decoded_attestation_subnets = attestation_result.unwrap().unwrap();

        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(1),
            Ok(true)
        ));
        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(5),
            Ok(true)
        ));
        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(0),
            Ok(false)
        ));
        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(10),
            Ok(false)
        ));

        let sync_committee_result =
            enr.get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY);
        assert!(
            sync_committee_result.is_some(),
            "Missing sync committee subnet field in ENR"
        );
        let decoded_sync_committee_subnets = sync_committee_result.unwrap().unwrap();

        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(0),
            Ok(true)
        ));
        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(2),
            Ok(true)
        ));
        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(1),
            Ok(false)
        ));
        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(3),
            Ok(false)
        ));
    }

    #[test]
    fn test_new_subnet_types() {
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.enable_attestation_subnet(3).unwrap();
        attestation_subnets.enable_attestation_subnet(42).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets
            .enable_sync_committee_subnet(0)
            .unwrap();
        sync_committee_subnets
            .enable_sync_committee_subnet(2)
            .unwrap();

        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        let attestation_result =
            enr.get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY);
        assert!(
            attestation_result.is_some(),
            "Missing attestation subnet field in ENR"
        );
        let decoded_attestation_subnets = attestation_result.unwrap().unwrap();

        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(3),
            Ok(true)
        ));
        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(42),
            Ok(true)
        ));
        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(0),
            Ok(false)
        ));
        assert!(matches!(
            decoded_attestation_subnets.is_attestation_subnet_enabled(10),
            Ok(false)
        ));

        let sync_committee_result =
            enr.get_decodable::<SyncCommitteeSubnets>(SYNC_COMMITTEE_BITFIELD_ENR_KEY);
        assert!(
            sync_committee_result.is_some(),
            "Missing sync committee subnet field in ENR"
        );
        let decoded_sync_committee_subnets = sync_committee_result.unwrap().unwrap();

        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(0),
            Ok(true)
        ));
        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(2),
            Ok(true)
        ));
        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(1),
            Ok(false)
        ));
        assert!(matches!(
            decoded_sync_committee_subnets.is_sync_committee_subnet_enabled(3),
            Ok(false)
        ));

        let attestation_subnet_predicate_fn = attestation_subnet_predicate(vec![3]);
        assert!(attestation_subnet_predicate_fn(&enr));

        let attestation_subnet_predicate_fn = attestation_subnet_predicate(vec![10]);
        assert!(!attestation_subnet_predicate_fn(&enr));

        let sync_committee_subnet_predicate_fn = sync_committee_subnet_predicate(vec![2]);
        assert!(sync_committee_subnet_predicate_fn(&enr));

        let sync_committee_subnet_predicate_fn = sync_committee_subnet_predicate(vec![1]);
        assert!(!sync_committee_subnet_predicate_fn(&enr));

        let combined_subnet_predicate_fn = |test_enr: &Enr| {
            attestation_subnet_predicate(vec![3])(test_enr)
                && sync_committee_subnet_predicate(vec![2])(test_enr)
        };
        assert!(combined_subnet_predicate_fn(&enr));

        let combined_subnet_predicate_fn = |test_enr: &Enr| {
            attestation_subnet_predicate(vec![10])(test_enr)
                && sync_committee_subnet_predicate(vec![1])(test_enr)
        };
        assert!(!combined_subnet_predicate_fn(&enr));
    }

    #[test]
    fn test_subnet_predicates() {
        let secret_key = SigningKey::random(&mut rand::thread_rng());
        let combined_key = CombinedKey::from(secret_key);

        let mut attestation_subnets = AttestationSubnets::new();
        attestation_subnets.enable_attestation_subnet(5).unwrap();

        let mut sync_committee_subnets = SyncCommitteeSubnets::new();
        sync_committee_subnets
            .enable_sync_committee_subnet(1)
            .unwrap();

        let enr = Enr::builder()
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_subnets)
            .add_value(SYNC_COMMITTEE_BITFIELD_ENR_KEY, &sync_committee_subnets)
            .build(&combined_key)
            .expect("Failed to build ENR");

        // Test attestation subnet predicate
        let attestation_subnet_predicate_fn = attestation_subnet_predicate(vec![5]);
        assert!(attestation_subnet_predicate_fn(&enr));

        // Test sync committee subnet predicate
        let sync_committee_subnet_predicate_fn = sync_committee_subnet_predicate(vec![1]);
        assert!(sync_committee_subnet_predicate_fn(&enr));

        // Test combined predicates
        let combined_subnet_predicate_fn = |enr: &Enr| {
            attestation_subnet_predicate(vec![5])(enr)
                && sync_committee_subnet_predicate(vec![1])(enr)
        };
        assert!(combined_subnet_predicate_fn(&enr));

        let combined_subnet_predicate_fn = |enr: &Enr| {
            attestation_subnet_predicate(vec![10])(enr)
                && sync_committee_subnet_predicate(vec![2])(enr)
        };
        assert!(!combined_subnet_predicate_fn(&enr));
    }
}
