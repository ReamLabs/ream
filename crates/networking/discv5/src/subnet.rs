use alloy_rlp::{BufMut, Decodable, Encodable, bytes::Bytes};
use anyhow::anyhow;
use discv5::Enr;
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitVector, typenum::U64};
use tracing::trace;

pub const ATTESTATION_BITFIELD_ENR_KEY: &str = "attnets";

#[derive(Clone, Debug, PartialEq)]
pub enum Subnet {
    Attestation(u8),
    SyncCommittee(u8),
}

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct Subnets {
    attestation_bits: Option<BitVector<U64>>,
}

impl Subnets {
    pub fn new() -> Self {
        Self {
            attestation_bits: None,
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
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
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
}

impl Encodable for Subnets {
    fn encode(&self, out: &mut dyn BufMut) {
        let ssz_bytes = self.as_ssz_bytes();
        let bytes = Bytes::from(ssz_bytes);
        bytes.encode(out);
    }
}

impl Decodable for Subnets {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;
        let subnets = Subnets::from_ssz_bytes(&bytes).map_err(|err| {
            alloy_rlp::Error::Custom(Box::leak(
                format!("Failed to decode SSZ subnets: {err:?}").into_boxed_str(),
            ))
        })?;
        Ok(subnets)
    }
}

pub fn subnet_predicate(subnets: Vec<Subnet>) -> impl Fn(&Enr) -> bool + Send + Sync {
    move |enr: &Enr| {
        let Some(Ok(subnets_state)) = enr.get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
        else {
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
                        Err(err) => {
                            trace!(
                                ?err,
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
