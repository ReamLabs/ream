use std::time::Instant;

use libp2p::{Multiaddr, PeerId};
use ream_consensus_lean::checkpoint::Checkpoint;
use ream_peer::{ConnectionState, Direction};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CachedPeer {
    /// libp2p peer ID
    pub peer_id: PeerId,

    /// Last known multiaddress observed for the peer
    pub last_seen_p2p_address: Option<Multiaddr>,

    /// Current known connection state
    pub state: ConnectionState,

    /// Direction of the most recent connection (inbound/outbound)
    pub direction: Direction,

    /// Last time we received a message from this peer
    #[serde(with = "instant_serde")]
    pub last_seen: Instant,

    pub head_checkpoint: Option<Checkpoint>,
    pub finalized_checkpoint: Option<Checkpoint>,
}

impl CachedPeer {
    pub fn new(
        peer_id: PeerId,
        address: Option<Multiaddr>,
        state: ConnectionState,
        direction: Direction,
    ) -> Self {
        CachedPeer {
            peer_id,
            last_seen_p2p_address: address,
            state,
            direction,
            last_seen: Instant::now(),
            head_checkpoint: None,
            finalized_checkpoint: None,
        }
    }

    /// Update the last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }
}

mod instant_serde {
    use std::time::{Duration, Instant};

    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = instant.elapsed();
        duration.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let duration = Duration::deserialize(deserializer)?;
        let now = Instant::now();
        let instant = now
            .checked_sub(duration)
            .ok_or_else(|| Error::custom("Errer checked_add"))?;
        Ok(instant)
    }
}
