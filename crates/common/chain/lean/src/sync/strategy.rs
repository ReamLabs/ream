use std::{env, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NearHeadBackfillStrategy {
    RequestOnly,
    GossipPreferred,
}

impl NearHeadBackfillStrategy {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("request-only") {
            Some(Self::RequestOnly)
        } else if value.eq_ignore_ascii_case("gossip-preferred") {
            Some(Self::GossipPreferred)
        } else {
            None
        }
    }

    pub fn from_env() -> Self {
        env::var("REAM_LEAN_BACKFILL_AB_STRATEGY")
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or(Self::GossipPreferred)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NearHeadFanoutStrategy {
    SinglePeer,
    DualPeer,
    DelayedHedge,
}

impl NearHeadFanoutStrategy {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("single-peer") {
            Some(Self::SinglePeer)
        } else if value.eq_ignore_ascii_case("dual-peer") {
            Some(Self::DualPeer)
        } else if value.eq_ignore_ascii_case("delayed-hedge") {
            Some(Self::DelayedHedge)
        } else {
            None
        }
    }

    pub fn from_env() -> Self {
        env::var("REAM_LEAN_FANOUT_AB_STRATEGY")
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or(Self::SinglePeer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffStrategy {
    Legacy,
    RobustBridge,
}

impl HandoffStrategy {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("legacy") {
            Some(Self::Legacy)
        } else if value.eq_ignore_ascii_case("robust-bridge") {
            Some(Self::RobustBridge)
        } else {
            None
        }
    }

    pub fn from_env() -> Self {
        env::var("REAM_LEAN_HANDOFF_AB_STRATEGY")
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or(Self::RobustBridge)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillTimeoutStrategy {
    Fixed,
    AdaptiveGap,
}

impl BackfillTimeoutStrategy {
    const FIXED_TIMEOUT: Duration = Duration::from_secs(2);

    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("fixed") {
            Some(Self::Fixed)
        } else if value.eq_ignore_ascii_case("adaptive-gap") {
            Some(Self::AdaptiveGap)
        } else {
            None
        }
    }

    pub fn from_env() -> Self {
        env::var("REAM_LEAN_TIMEOUT_AB_STRATEGY")
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or(Self::AdaptiveGap)
    }

    pub fn timeout_for_peer_gap(self, peer_gap_slots: u64) -> Duration {
        match self {
            Self::Fixed => Self::FIXED_TIMEOUT,
            // Near-head: retry faster. Far from head: allow slower peers more time.
            Self::AdaptiveGap => {
                if peer_gap_slots <= 2 {
                    Duration::from_millis(750)
                } else if peer_gap_slots <= 8 {
                    Self::FIXED_TIMEOUT
                } else {
                    Duration::from_secs(4)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingRequestDedupStrategy {
    Legacy,
    Dedup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerSelectionStrategy {
    ScoreOnly,
    LatencyWeighted,
}

impl PeerSelectionStrategy {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("score-only") {
            Some(Self::ScoreOnly)
        } else if value.eq_ignore_ascii_case("latency-weighted") {
            Some(Self::LatencyWeighted)
        } else {
            None
        }
    }

    pub fn from_env() -> Self {
        env::var("REAM_LEAN_PEER_SELECT_AB_STRATEGY")
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or(Self::LatencyWeighted)
    }
}

impl PendingRequestDedupStrategy {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("legacy") {
            Some(Self::Legacy)
        } else if value.eq_ignore_ascii_case("dedup") {
            Some(Self::Dedup)
        } else {
            None
        }
    }

    pub fn from_env() -> Self {
        env::var("REAM_LEAN_PENDING_DEDUP_AB_STRATEGY")
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or(Self::Dedup)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffInputs {
    pub is_behind_peers: bool,
    pub has_pending_backfill_work: bool,
    pub has_near_head_bridge: bool,
    pub has_active_backfill_jobs: bool,
    pub has_inflight_backfill_requests: bool,
}

pub fn should_switch_to_synced(strategy: HandoffStrategy, inputs: HandoffInputs) -> bool {
    if inputs.is_behind_peers {
        return false;
    }

    match strategy {
        HandoffStrategy::Legacy => true,
        HandoffStrategy::RobustBridge => {
            if !inputs.has_pending_backfill_work {
                return true;
            }

            // Allow bridge-based handoff only when remaining work is passive bookkeeping.
            inputs.has_near_head_bridge
                && !inputs.has_active_backfill_jobs
                && !inputs.has_inflight_backfill_requests
        }
    }
}

pub fn should_fanout_near_head(
    strategy: NearHeadFanoutStrategy,
    peer_gap_slots: u64,
    max_near_head_gap_slots: u64,
) -> bool {
    matches!(
        strategy,
        NearHeadFanoutStrategy::DualPeer | NearHeadFanoutStrategy::DelayedHedge
    ) && peer_gap_slots > 0
        && peer_gap_slots <= max_near_head_gap_slots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_strategy_legacy_ignores_pending_backfill_work() {
        assert!(should_switch_to_synced(
            HandoffStrategy::Legacy,
            HandoffInputs {
                is_behind_peers: false,
                has_pending_backfill_work: true,
                has_near_head_bridge: false,
                has_active_backfill_jobs: true,
                has_inflight_backfill_requests: true,
            },
        ));
    }

    #[test]
    fn handoff_strategy_robust_requires_bridge_and_no_active_work() {
        assert!(!should_switch_to_synced(
            HandoffStrategy::RobustBridge,
            HandoffInputs {
                is_behind_peers: false,
                has_pending_backfill_work: true,
                has_near_head_bridge: false,
                has_active_backfill_jobs: true,
                has_inflight_backfill_requests: true,
            },
        ));

        assert!(!should_switch_to_synced(
            HandoffStrategy::RobustBridge,
            HandoffInputs {
                is_behind_peers: false,
                has_pending_backfill_work: true,
                has_near_head_bridge: true,
                has_active_backfill_jobs: true,
                has_inflight_backfill_requests: false,
            },
        ));

        assert!(!should_switch_to_synced(
            HandoffStrategy::RobustBridge,
            HandoffInputs {
                is_behind_peers: false,
                has_pending_backfill_work: true,
                has_near_head_bridge: true,
                has_active_backfill_jobs: false,
                has_inflight_backfill_requests: true,
            },
        ));

        assert!(should_switch_to_synced(
            HandoffStrategy::RobustBridge,
            HandoffInputs {
                is_behind_peers: false,
                has_pending_backfill_work: true,
                has_near_head_bridge: true,
                has_active_backfill_jobs: false,
                has_inflight_backfill_requests: false,
            },
        ));
    }

    #[test]
    fn handoff_stays_syncing_when_behind_peers() {
        assert!(!should_switch_to_synced(
            HandoffStrategy::Legacy,
            HandoffInputs {
                is_behind_peers: true,
                has_pending_backfill_work: false,
                has_near_head_bridge: true,
                has_active_backfill_jobs: false,
                has_inflight_backfill_requests: false,
            },
        ));
    }

    #[test]
    fn strategy_parsing_supports_ab_variants() {
        assert_eq!(
            NearHeadBackfillStrategy::parse("request-only"),
            Some(NearHeadBackfillStrategy::RequestOnly)
        );
        assert_eq!(
            NearHeadBackfillStrategy::parse("gossip-preferred"),
            Some(NearHeadBackfillStrategy::GossipPreferred)
        );
        assert_eq!(
            NearHeadFanoutStrategy::parse("single-peer"),
            Some(NearHeadFanoutStrategy::SinglePeer)
        );
        assert_eq!(
            NearHeadFanoutStrategy::parse("dual-peer"),
            Some(NearHeadFanoutStrategy::DualPeer)
        );
        assert_eq!(
            NearHeadFanoutStrategy::parse("delayed-hedge"),
            Some(NearHeadFanoutStrategy::DelayedHedge)
        );
        assert_eq!(
            HandoffStrategy::parse("legacy"),
            Some(HandoffStrategy::Legacy)
        );
        assert_eq!(
            HandoffStrategy::parse("robust-bridge"),
            Some(HandoffStrategy::RobustBridge)
        );
        assert_eq!(
            BackfillTimeoutStrategy::parse("fixed"),
            Some(BackfillTimeoutStrategy::Fixed)
        );
        assert_eq!(
            BackfillTimeoutStrategy::parse("adaptive-gap"),
            Some(BackfillTimeoutStrategy::AdaptiveGap)
        );
        assert_eq!(
            PendingRequestDedupStrategy::parse("legacy"),
            Some(PendingRequestDedupStrategy::Legacy)
        );
        assert_eq!(
            PendingRequestDedupStrategy::parse("dedup"),
            Some(PendingRequestDedupStrategy::Dedup)
        );
        assert_eq!(
            PeerSelectionStrategy::parse("score-only"),
            Some(PeerSelectionStrategy::ScoreOnly)
        );
        assert_eq!(
            PeerSelectionStrategy::parse("latency-weighted"),
            Some(PeerSelectionStrategy::LatencyWeighted)
        );
    }

    #[test]
    fn adaptive_timeout_ab_behaves_as_expected() {
        assert_eq!(
            BackfillTimeoutStrategy::Fixed.timeout_for_peer_gap(1),
            Duration::from_secs(2)
        );
        assert_eq!(
            BackfillTimeoutStrategy::AdaptiveGap.timeout_for_peer_gap(1),
            Duration::from_millis(750)
        );
        assert_eq!(
            BackfillTimeoutStrategy::AdaptiveGap.timeout_for_peer_gap(6),
            Duration::from_secs(2)
        );
        assert_eq!(
            BackfillTimeoutStrategy::AdaptiveGap.timeout_for_peer_gap(30),
            Duration::from_secs(4)
        );
    }

    #[test]
    fn near_head_fanout_ab_behaves_as_expected() {
        assert!(!should_fanout_near_head(
            NearHeadFanoutStrategy::SinglePeer,
            1,
            4
        ));
        assert!(should_fanout_near_head(
            NearHeadFanoutStrategy::DualPeer,
            1,
            4
        ));
        assert!(!should_fanout_near_head(
            NearHeadFanoutStrategy::DualPeer,
            0,
            4
        ));
        assert!(!should_fanout_near_head(
            NearHeadFanoutStrategy::DualPeer,
            9,
            4
        ));
        assert!(should_fanout_near_head(
            NearHeadFanoutStrategy::DelayedHedge,
            2,
            4
        ));
    }
}
