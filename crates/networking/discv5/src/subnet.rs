use alloy_rlp::{BufMut, Decodable, Encodable, bytes::Bytes};
use anyhow::anyhow;
use discv5::Enr;
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitVector, typenum::U64};
use tracing::error;

pub const ATTESTATION_BITFIELD_ENR_KEY: &str = "attnets";

#[derive(Clone, Debug, PartialEq)]
pub enum Subnet {
    Attestation(u8),
    SyncCommittee(u8),
}

#[derive(Clone, Debug, Default, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct Subnets {
    attestation_bits: BitVector<U64>,
}

impl Subnets {
    pub fn new() -> Self {
        Self {
            attestation_bits: BitVector::new(),
        }
    }

    pub fn enable_subnet(&mut self, subnet: Subnet) -> anyhow::Result<()> {
        match subnet {
            Subnet::Attestation(id) if id < 64 => {
                self.attestation_bits
                    .set(id as usize, true)
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
                self.attestation_bits
                    .set(id as usize, false)
                    .map_err(|err| anyhow!("Subnet ID out of bounds: {err:?}"))?;
                Ok(())
            }
            Subnet::Attestation(_) => Ok(()),
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
        }
    }

    pub fn is_active(&self, subnet: Subnet) -> anyhow::Result<bool> {
        let active = match subnet {
            Subnet::Attestation(id) if id < 64 => self
                .attestation_bits
                .get(id as usize)
                .map_err(|err| anyhow!("Couldn't get expected attestation {:?}", err))?,
            Subnet::Attestation(_) => false,
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
        };
        Ok(active)
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
        let Some(subnets_state) = (match enr
            .get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .transpose()
        {
            Ok(subnets_state) => subnets_state,
            Err(err) => {
                error!("Could not get subnets_state {err:?}, {}", enr.to_base64());
                return false;
            }
        }) else {
            return false;
        };

        let mut matches_subnet = false;
        for subnet in &subnets {
            match subnet {
                Subnet::Attestation(id) => {
                    if *id >= 64 {
                        error!(
                            "Peer rejected: subnet ID {} exceeds attestation bitfield length; peer_id: {}",
                            id,
                            enr.node_id()
                        );
                        return false;
                    }
                    matches_subnet |= match subnets_state.attestation_bits.get(*id as usize) {
                        Ok(true) => true,
                        Ok(false) => {
                            error!(
                                "Peer found but not on subnet {}; peer_id: {}",
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
                Subnet::SyncCommittee(_) => {
                    unimplemented!("SyncCommittee support not yet implemented")
                }
            }
        }
        matches_subnet
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use discv5::Enr;

    use super::*;

    #[test]
    fn test_decodes_subnets() {
        let enr = Enr::from_str("enr:-LS4QLe5eq5PFn1ZynqkrF6yg6ZGoplSDSNEPXtXfQh0vqhrDBQZICVoQu-AdeBOmtOFcAO7a0tJLdSlqStkdxkXnwaCCKSHYXR0bmV0c4gAAAAAAAAAMIRldGgykGqVoakEAAAA__________-CaWSCdjSCaXCEywwIqolzZWNwMjU2azGhA2JDBvnFqwtkUx34b_OdHXN1eO2JBMLWbzZXfGksk3YRg3RjcIIjkYN1ZHCCI5E").unwrap();
        assert_eq!(
            enr.get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
                .unwrap()
                .unwrap()
                .attestation_bits,
            BitVector::from_bytes(vec![0, 0, 0, 0, 0, 0, 0, 48].into()).unwrap()
        );
        let enr = Enr::from_str("enr:-Ly4QHiJW24IzegmekAp3SRXhmopPLG-6PI7e-poXLDeaTcJC0yUtwg3XYELsw8v1-GkBByYpw6IaYDbtiaZLbwaOXUeh2F0dG5ldHOI__________-EZXRoMpBqlaGpBAAAAP__________gmlkgnY0gmlwhMb05QKJc2VjcDI1NmsxoQIMnwShvit2bpXbH0iPB3uyaPYTQ_dYOFl6TNp2h01zZohzeW5jbmV0cw-DdGNwgiMog3VkcIIjKA").unwrap();
        assert_eq!(
            enr.get_decodable::<Subnets>(ATTESTATION_BITFIELD_ENR_KEY)
                .unwrap()
                .unwrap()
                .attestation_bits,
            BitVector::from_bytes(vec![255; 8].into()).unwrap()
        );
    }
}
