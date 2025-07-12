use alloy_primitives::{B256, Bytes, aliases::B32, bytes};
use alloy_rlp::{Decodable, Encodable};
use ream_consensus_misc::{constants::FAR_FUTURE_EPOCH, fork_data::ForkData};
use ream_network_spec::networks::network_spec;
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

pub const ENR_ETH2_KEY: &str = "eth2";

#[derive(Default, Debug, Encode, Decode)]
pub struct EnrForkId {
    pub fork_digest: B32,
    pub next_fork_version: B32,
    pub next_fork_epoch: u64,
}

impl EnrForkId {
    pub fn electra(genesis_validators_root: B256) -> Self {
        let current_fork_version = network_spec().electra_fork_version;
        let next_fork_version = current_fork_version;
        let next_fork_epoch = FAR_FUTURE_EPOCH;

        let fork_digest = ForkData {
            current_version: current_fork_version,
            genesis_validators_root,
        }
        .compute_fork_digest();

        Self {
            fork_digest,
            next_fork_version,
            next_fork_epoch,
        }
    }
}

impl Encodable for EnrForkId {
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        let ssz_bytes = self.as_ssz_bytes();
        let bytes = Bytes::from(ssz_bytes);
        bytes.encode(out);
    }
}

impl Decodable for EnrForkId {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;
        let enr_fork_id = EnrForkId::from_ssz_bytes(&bytes)
            .map_err(|_| alloy_rlp::Error::Custom("Failed to decode SSZ ENRForkID"))?;
        Ok(enr_fork_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let fork_id = EnrForkId {
            fork_digest: B32::from_slice(&[1, 2, 3, 4]),
            next_fork_version: B32::from_slice(&[5, 6, 7, 8]),
            next_fork_epoch: 100,
        };

        let mut buffer = Vec::new();
        fork_id.encode(&mut buffer);
        let mut rlp_bytes_slice = buffer.as_slice();
        let deserialized = EnrForkId::decode(&mut rlp_bytes_slice)?;

        assert_eq!(fork_id.fork_digest, deserialized.fork_digest);
        assert_eq!(fork_id.next_fork_version, deserialized.next_fork_version);
        assert_eq!(fork_id.next_fork_epoch, deserialized.next_fork_epoch);
        Ok(())
    }
}
