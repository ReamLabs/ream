use alloy_primitives::{B256, aliases::B32};
use alloy_rlp::{BufMut, Decodable, Encodable, bytes::Bytes};
use ream_consensus_misc::constants::beacon::FAR_FUTURE_EPOCH;
use ream_network_spec::networks::beacon_network_spec;
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};
use tracing::warn;

pub const ENR_ETH2_KEY: &str = "eth2";

#[derive(Default, Debug, Encode, Decode)]
pub struct EnrForkId {
    pub fork_digest: B32,
    pub next_fork_version: B32,
    pub next_fork_epoch: u64,
}

impl EnrForkId {
    pub fn current(genesis_validators_root: B256, epoch: u64) -> Self {
        let spec = beacon_network_spec();

        let fork_digest = spec.fork_digest(epoch, genesis_validators_root);

        let fork_schedule = spec.fork_schedule();

        let current_version = spec.current_fork_version(epoch);

        let next_regular_fork = fork_schedule.0.iter().find(|fork| fork.epoch > epoch);

        let next_bpo_epoch = spec
            .blob_schedule
            .iter()
            .map(|params| params.epoch)
            .filter(|&bpo_epoch| bpo_epoch > epoch)
            .min();

        let (next_fork_version, next_fork_epoch) = match (next_regular_fork, next_bpo_epoch) {
            (Some(regular), Some(bpo)) if regular.epoch <= bpo => {
                (regular.current_version, regular.epoch)
            }
            (Some(regular), None) => (regular.current_version, regular.epoch),
            (_, Some(bpo)) => (current_version, bpo),
            (None, None) => (current_version, FAR_FUTURE_EPOCH),
        };

        Self {
            fork_digest,
            next_fork_version,
            next_fork_epoch,
        }
    }
}

impl Encodable for EnrForkId {
    fn encode(&self, out: &mut dyn BufMut) {
        let ssz_bytes = self.as_ssz_bytes();
        let bytes = Bytes::from(ssz_bytes);
        bytes.encode(out);
    }
}

impl Decodable for EnrForkId {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = Bytes::decode(buf)?;
        let enr_fork_id = EnrForkId::from_ssz_bytes(&bytes).map_err(|err| {
            warn!("Failed to decode SSZ ENRForkID: {err:?}");
            alloy_rlp::Error::Custom("Failed to decode SSZ ENRForkID")
        })?;
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
