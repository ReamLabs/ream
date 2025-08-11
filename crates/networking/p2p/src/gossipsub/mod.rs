//! https://ethereum.github.io/consensus-specs/specs/phase0/p2p-interface/#the-gossip-domain-gossipsub

pub mod beacon;
pub mod error;
pub mod lean;
pub mod snappy;

use libp2p::gossipsub::{AllowAllSubscriptionFilter, Behaviour};

use crate::gossipsub::snappy::SnappyTransform;

pub type GossipsubBehaviour = Behaviour<SnappyTransform, AllowAllSubscriptionFilter>;
