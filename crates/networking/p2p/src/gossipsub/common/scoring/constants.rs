//! Common Gossipsub Peer Scoring Constants
//!
//! These values are based on the Ethereum consensus specification:
//! https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/p2p-interface.md#peer-scoring
//!
//! These constants are shared between beacon and lean networks to ensure
//! consistent peer management and protection against misbehaving peers.

// ============================================================================
// Global Peer Score Parameters
// ============================================================================

/// Score decays to zero threshold
pub const DECAY_TO_ZERO: f64 = 0.01;

/// Application-specific scoring weight
pub const APP_SPECIFIC_WEIGHT: f64 = 1.0;

/// IP colocation factor weight (penalty for multiple peers from same IP)
pub const IP_COLOCATION_FACTOR_WEIGHT: f64 = -35.11;

/// IP colocation factor threshold (max peers from same IP before penalty)
pub const IP_COLOCATION_FACTOR_THRESHOLD: f64 = 10.0;

/// Behaviour penalty weight
pub const BEHAVIOUR_PENALTY_WEIGHT: f64 = -15.92;

/// Behaviour penalty threshold
pub const BEHAVIOUR_PENALTY_THRESHOLD: f64 = 6.0;

/// Behaviour penalty decay time in epochs
pub const BEHAVIOUR_PENALTY_DECAY_EPOCHS: f64 = 10.0;

// ============================================================================
// Peer Score Thresholds
// ============================================================================

/// Peers below this score will not receive gossip messages
pub const GOSSIP_THRESHOLD: f64 = -4000.0;

/// Peers below this score will not receive published messages
pub const PUBLISH_THRESHOLD: f64 = -8000.0;

/// Peers below this score are disconnected (graylisted)
pub const GRAYLIST_THRESHOLD: f64 = -16000.0;

/// Only accept peer exchange from peers above this threshold
pub const ACCEPT_PX_THRESHOLD: f64 = 100.0;

/// Only opportunistically graft peers above this threshold
pub const OPPORTUNISTIC_GRAFT_THRESHOLD: f64 = 5.0;
