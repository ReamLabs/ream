use alloy_rlp::bytes::Bytes;
use discv5::Enr;
use ssz::Decode;
use ssz_types::{BitVector, typenum::U64};
use tracing::trace;

pub const ATTESTATION_BITFIELD_ENR_KEY: &str = "attnets";

#[derive(Clone, Debug, PartialEq)]
pub enum Subnet {
    Attestation(u8),
    SyncCommittee(u8),
}

#[derive(Clone, Debug, Default)]
pub struct Subnets {
    attestation_bits: Option<BitVector<U64>>,
}

impl Subnets {
    pub fn new() -> Self {
        Self {
            attestation_bits: None,
        }
    }

    pub fn enable_subnet(&mut self, subnet: Subnet) {
        match subnet {
            Subnet::Attestation(id) if id < 64 => {
                let bits = self.attestation_bits.get_or_insert(BitVector::new());
                bits.set(id as usize, true)
                    .expect("Subnet ID within bounds");
            }
            Subnet::Attestation(_) => {}
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
        }
    }

    pub fn disable_subnet(&mut self, subnet: Subnet) {
        match subnet {
            Subnet::Attestation(id) if id < 64 => {
                if let Some(bits) = &mut self.attestation_bits {
                    bits.set(id as usize, false)
                        .expect("Subnet ID within bounds");
                }
            }
            Subnet::Attestation(_) => {}
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
        }
    }

    pub fn is_active(&self, subnet: Subnet) -> bool {
        match subnet {
            Subnet::Attestation(id) if id < 64 => self
                .attestation_bits
                .as_ref()
                .is_some_and(|bits| bits.get(id as usize).unwrap_or(false)),
            Subnet::Attestation(_) => false,
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
        }
    }

    pub fn attestation_bytes(&self) -> Option<Vec<u8>> {
        self.attestation_bits
            .as_ref()
            .map(|bits| bits.clone().into_bytes().into_vec())
    }

    pub fn from_enr(enr: &Enr) -> Self {
        let attestation_bits = enr
            .get_decodable(ATTESTATION_BITFIELD_ENR_KEY)
            .and_then(|res| res.ok())
            .and_then(|bytes: Bytes| BitVector::<U64>::from_ssz_bytes(&bytes).ok());

        Self { attestation_bits }
    }
}

pub fn subnet_predicate(subnets: Vec<Subnet>) -> impl Fn(&Enr) -> bool + Send + Sync {
    move |enr: &Enr| {
        let subnets_state = Subnets::from_enr(enr);
        let attestation_bits = match &subnets_state.attestation_bits {
            Some(bits) => bits,
            None => {
                trace!(
                    "Peer rejected: invalid or missing attnets field; peer_id: {}",
                    enr.node_id()
                );
                return false;
            }
        };

        let mut matches_subnet = false;
        for subnet in &subnets {
            match subnet {
                Subnet::Attestation(id) => {
                    if *id >= 64 {
                        trace!(
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
                                "Peer found but not on subnet {}; peer_id: {}",
                                id,
                                enr.node_id()
                            );
                            false
                        }
                        Err(_) => {
                            trace!(
                                "Peer rejected: invalid attestation bitfield index; subnet_id: {}, peer_id: {}",
                                id,
                                enr.node_id()
                            );
                            false
                        }
                    };
                }
                Subnet::SyncCommittee(_) => {
                    unimplemented!("SyncCommittee support not yet implemented")
                }
            }
        }
        matches_subnet
    }
}
