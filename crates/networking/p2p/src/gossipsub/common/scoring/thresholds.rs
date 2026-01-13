use libp2p::gossipsub::PeerScoreThresholds;

use super::constants::{
    ACCEPT_PX_THRESHOLD, GOSSIP_THRESHOLD, GRAYLIST_THRESHOLD, OPPORTUNISTIC_GRAFT_THRESHOLD,
    PUBLISH_THRESHOLD,
};

/// Build peer score thresholds for gossipsub.
///
/// These thresholds determine how peers are treated based on their score:
/// - Below gossip_threshold: stop gossiping to this peer
/// - Below publish_threshold: stop publishing to this peer
/// - Below graylist_threshold: disconnect and ignore this peer
/// - Above accept_px_threshold: accept peer exchange from this peer
/// - Above opportunistic_graft_threshold: allow opportunistic grafting
pub fn build_peer_score_thresholds() -> PeerScoreThresholds {
    PeerScoreThresholds {
        gossip_threshold: GOSSIP_THRESHOLD,
        publish_threshold: PUBLISH_THRESHOLD,
        graylist_threshold: GRAYLIST_THRESHOLD,
        accept_px_threshold: ACCEPT_PX_THRESHOLD,
        opportunistic_graft_threshold: OPPORTUNISTIC_GRAFT_THRESHOLD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thresholds_are_ordered_correctly() {
        let thresholds = build_peer_score_thresholds();

        // Thresholds should be in order: graylist < publish < gossip < 0
        assert!(thresholds.graylist_threshold < thresholds.publish_threshold);
        assert!(thresholds.publish_threshold < thresholds.gossip_threshold);
        assert!(thresholds.gossip_threshold < 0.0);

        // Positive thresholds should be above 0
        assert!(thresholds.accept_px_threshold > 0.0);
        assert!(thresholds.opportunistic_graft_threshold > 0.0);
    }
}
