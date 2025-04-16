use alloy_primitives::{B256, Bytes, aliases::B32, bytes, fixed_bytes};
use alloy_rlp::{Decodable, Encodable};
use ream_consensus::{constants::FAR_FUTURE_EPOCH, fork_data::ForkData};
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

pub const ENR_ETH2_KEY: &str = "eth2";
pub const GENESIS_VALIDATORS_ROOT: B256 =
    fixed_bytes!("0x0000000000000000000000000000000000000000000000000000000000000000");

#[derive(Default, Debug, Encode, Decode)]
pub struct ENRForkID {
    pub fork_digest: B32,
    pub next_fork_version: B32,
    pub next_fork_epoch: u64,
}

impl ENRForkID {
    pub fn pectra() -> Self {
        let current_fork_version = fixed_bytes!("0x05000000");
        let next_fork_version = current_fork_version;
        let next_fork_epoch = FAR_FUTURE_EPOCH;

        let fork_digest = ForkData {
            current_version: current_fork_version,
            genesis_validators_root: GENESIS_VALIDATORS_ROOT,
        }
        .compute_fork_digest();

        Self {
            fork_digest,
            next_fork_version,
            next_fork_epoch,
        }
    }
}

impl Encodable for ENRForkID {
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        let ssz_bytes = self.as_ssz_bytes();
        let bytes = Bytes::from(ssz_bytes);
        bytes.encode(out);
    }
}

impl Decodable for ENRForkID {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;
        let enr_fork_id = ENRForkID::from_ssz_bytes(&bytes)
            .map_err(|_| alloy_rlp::Error::Custom("Failed to decode SSZ ENRForkID"))?;
        Ok(enr_fork_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let fork_id = ENRForkID {
            fork_digest: B32::from_slice(&[1, 2, 3, 4]),
            next_fork_version: B32::from_slice(&[5, 6, 7, 8]),
            next_fork_epoch: 100,
        };

        let mut buffer = Vec::new();
        fork_id.encode(&mut buffer);
        let mut rlp_bytes_slice = buffer.as_slice();
        let deserialized = ENRForkID::decode(&mut rlp_bytes_slice)?;

        assert_eq!(fork_id.fork_digest, deserialized.fork_digest);
        assert_eq!(fork_id.next_fork_version, deserialized.next_fork_version);
        assert_eq!(fork_id.next_fork_epoch, deserialized.next_fork_epoch);
        Ok(())
    }
}
