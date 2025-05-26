//! https://ethereum.github.io/consensus-specs/specs/phase0/p2p-interface/#the-gossip-domain-gossipsub

pub mod configurations;
pub mod error;
pub mod message;
pub mod snappy;
pub mod topics;

use libp2p::gossipsub::{AllowAllSubscriptionFilter, Behaviour};
use snappy::SnappyTransform;

pub type GossipsubBehaviour = Behaviour<SnappyTransform, AllowAllSubscriptionFilter>;
