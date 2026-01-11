use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use libp2p::PeerId;
use parking_lot::RwLock;
use ream_storage::tables::banned_peers::{BannedPeerEntry, BannedPeersTable};
use tracing::{info, warn};

/// Default ban duration in seconds (1 hour).
pub const DEFAULT_BAN_DURATION_SECS: u64 = 3600;

/// Reason for banning a peer.
#[derive(Debug, Clone)]
pub enum BanReason {
    /// Peer scored below the graylist threshold.
    LowScore(f64),
    /// Peer sent too many invalid messages.
    InvalidMessages(u64),
    /// Peer violated the protocol.
    ProtocolViolation(String),
    /// Peer failed during sync operations.
    SyncFailure(String),
    /// Manually banned by operator.
    Manual(String),
}

impl std::fmt::Display for BanReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BanReason::LowScore(score) => write!(f, "Low score: {score:.2}"),
            BanReason::InvalidMessages(count) => write!(f, "Invalid messages: {count}"),
            BanReason::ProtocolViolation(reason) => write!(f, "Protocol violation: {reason}"),
            BanReason::SyncFailure(reason) => write!(f, "Sync failure: {reason}"),
            BanReason::Manual(reason) => write!(f, "Manual ban: {reason}"),
        }
    }
}

/// Manages peer scores and banning.
///
/// This manager maintains an in-memory cache of banned peers for fast lookup,
/// and optionally persists bans to the database for persistence across restarts.
pub struct PeerScoreManager {
    /// Currently banned peers (in-memory cache for fast lookups).
    banned_peers: RwLock<HashSet<PeerId>>,
    /// Default ban duration.
    ban_duration: Duration,
    /// Last time expired bans were cleaned up.
    last_cleanup: RwLock<Instant>,
    /// Cleanup interval (check for expired bans periodically).
    cleanup_interval: Duration,
    /// Optional database for persistence.
    banned_peers_table: Option<Arc<BannedPeersTable>>,
}

impl Default for PeerScoreManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(DEFAULT_BAN_DURATION_SECS), None)
    }
}

impl PeerScoreManager {
    /// Create a new PeerScoreManager with the specified ban duration and optional database.
    pub fn new(ban_duration: Duration, banned_peers_table: Option<Arc<BannedPeersTable>>) -> Self {
        Self {
            banned_peers: RwLock::new(HashSet::new()),
            ban_duration,
            last_cleanup: RwLock::new(Instant::now()),
            cleanup_interval: Duration::from_secs(300), // Check every 5 minutes
            banned_peers_table,
        }
    }

    /// Load banned peers from the database on startup.
    ///
    /// This populates the in-memory cache with persisted bans that haven't expired.
    /// Returns the number of valid bans loaded.
    pub fn load_from_db(&self) -> anyhow::Result<usize> {
        let Some(ref table) = self.banned_peers_table else {
            return Ok(0);
        };

        let all_entries = table.get_all()?;
        let mut loaded_count = 0;

        let mut banned = self.banned_peers.write();
        for (peer_id_bytes, entry) in all_entries {
            if entry.is_expired() {
                // Remove expired bans during load
                if let Err(err) = table.remove(&peer_id_bytes) {
                    warn!("Failed to remove expired ban: {err}");
                }
                continue;
            }

            // Try to parse the peer ID from bytes
            if let Ok(peer_id) = PeerId::from_bytes(&peer_id_bytes) {
                banned.insert(peer_id);
                loaded_count += 1;
            } else {
                warn!("Failed to parse peer ID from database, removing entry");
                if let Err(err) = table.remove(&peer_id_bytes) {
                    warn!("Failed to remove invalid peer ID entry: {err}");
                }
            }
        }

        info!("Loaded {loaded_count} banned peers from database");
        Ok(loaded_count)
    }

    /// Ban a peer with the specified reason.
    ///
    /// The ban is added to the in-memory cache and persisted to the database if available.
    pub fn ban_peer(&self, peer_id: PeerId, reason: BanReason) -> anyhow::Result<()> {
        let reason_str = reason.to_string();

        // Add to in-memory cache
        self.banned_peers.write().insert(peer_id);

        // Persist to database if available
        if let Some(ref table) = self.banned_peers_table {
            let entry = BannedPeerEntry::new(&reason_str, Some(self.ban_duration.as_secs()));
            table.insert(&peer_id.to_bytes(), &entry)?;
            info!("Banned peer {peer_id}: {reason_str} (persisted to database)");
        } else {
            info!("Banned peer {peer_id}: {reason_str} (in-memory only)");
        }

        Ok(())
    }

    /// Ban a peer in memory only (no database persistence).
    ///
    /// This is useful for networks that don't have database access in their event loop.
    /// The ban will last until the node restarts or the ban expires.
    pub fn ban_peer_memory_only(&self, peer_id: PeerId) {
        self.banned_peers.write().insert(peer_id);
        info!("Banned peer {peer_id} in memory only");
    }

    /// Ban a peer permanently (no expiration).
    pub fn ban_peer_permanently(&self, peer_id: PeerId, reason: BanReason) -> anyhow::Result<()> {
        let reason_str = reason.to_string();

        self.banned_peers.write().insert(peer_id);

        if let Some(ref table) = self.banned_peers_table {
            let entry = BannedPeerEntry::new(&reason_str, None); // None = permanent
            table.insert(&peer_id.to_bytes(), &entry)?;
            info!("Permanently banned peer {peer_id}: {reason_str} (persisted to database)");
        } else {
            info!("Permanently banned peer {peer_id}: {reason_str} (in-memory only)");
        }

        Ok(())
    }

    /// Unban a peer.
    pub fn unban_peer(&self, peer_id: &PeerId) -> anyhow::Result<bool> {
        let was_banned = self.banned_peers.write().remove(peer_id);

        if was_banned {
            if let Some(ref table) = self.banned_peers_table {
                table.remove(&peer_id.to_bytes())?;
            }
            info!("Unbanned peer {peer_id}");
        }

        Ok(was_banned)
    }

    /// Check if a peer is banned.
    ///
    /// This is a fast in-memory lookup.
    pub fn is_banned(&self, peer_id: &PeerId) -> bool {
        self.banned_peers.read().contains(peer_id)
    }

    /// Get the number of currently banned peers.
    pub fn banned_count(&self) -> usize {
        self.banned_peers.read().len()
    }

    /// Get all banned peer IDs.
    pub fn get_banned_peers(&self) -> Vec<PeerId> {
        self.banned_peers.read().iter().copied().collect()
    }

    /// Cleanup expired bans from the database and in-memory cache.
    ///
    /// This should be called periodically (e.g., every few minutes).
    pub fn cleanup_expired_bans(&self) -> anyhow::Result<usize> {
        let Some(ref table) = self.banned_peers_table else {
            return Ok(0);
        };

        // Check if we need to run cleanup
        {
            let last = *self.last_cleanup.read();
            if last.elapsed() < self.cleanup_interval {
                return Ok(0);
            }
        }

        // Update last cleanup time
        *self.last_cleanup.write() = Instant::now();

        // Get all entries from database and check for expired ones
        let all_entries = table.get_all()?;
        let mut removed_count = 0;

        let mut banned = self.banned_peers.write();
        for (peer_id_bytes, entry) in all_entries {
            if entry.is_expired() {
                // Remove from database
                table.remove(&peer_id_bytes)?;

                // Remove from in-memory cache
                if let Ok(peer_id) = PeerId::from_bytes(&peer_id_bytes) {
                    banned.remove(&peer_id);
                }

                removed_count += 1;
            }
        }

        if removed_count > 0 {
            info!("Cleaned up {removed_count} expired bans");
        }

        Ok(removed_count)
    }

    /// Force cleanup of expired bans (ignores the cleanup interval).
    pub fn force_cleanup(&self) -> anyhow::Result<usize> {
        // Reset the last cleanup time to force immediate cleanup
        *self.last_cleanup.write() =
            Instant::now() - self.cleanup_interval - Duration::from_secs(1);
        self.cleanup_expired_bans()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ban_reason_display() {
        let reason = BanReason::LowScore(-5000.0);
        assert!(reason.to_string().contains("-5000"));

        let reason = BanReason::SyncFailure("timeout".to_string());
        assert!(reason.to_string().contains("timeout"));
    }

    #[test]
    fn test_peer_score_manager_memory_operations() {
        let manager = PeerScoreManager::default();
        let peer_id = PeerId::random();

        // Initially not banned
        assert!(!manager.is_banned(&peer_id));
        assert_eq!(manager.banned_count(), 0);

        // Add to banned set directly (without database)
        manager.banned_peers.write().insert(peer_id);

        // Now banned
        assert!(manager.is_banned(&peer_id));
        assert_eq!(manager.banned_count(), 1);

        // Get banned peers
        let banned = manager.get_banned_peers();
        assert_eq!(banned.len(), 1);
        assert!(banned.contains(&peer_id));

        // Remove from banned set
        manager.banned_peers.write().remove(&peer_id);
        assert!(!manager.is_banned(&peer_id));
        assert_eq!(manager.banned_count(), 0);
    }
}
