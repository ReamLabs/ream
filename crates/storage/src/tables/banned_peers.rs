use std::sync::Arc;

use redb::{
    Database, Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
};
use ssz::{Decode, Encode};
// Re-export for convenience
pub use ssz::{Decode as SszDecode, Encode as SszEncode};
use ssz_derive::{Decode, Encode};

use crate::errors::StoreError;

/// Entry for a banned peer stored in the database.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BannedPeerEntry {
    /// Unix timestamp when the peer was banned (seconds since epoch).
    pub banned_at: u64,
    /// Unix timestamp when the ban expires (0 = permanent ban).
    pub ban_expires_at: u64,
    /// Reason for the ban (truncated to 256 bytes).
    pub reason: Vec<u8>,
}

impl BannedPeerEntry {
    /// Create a new banned peer entry with a ban duration.
    pub fn new(reason: &str, ban_duration_secs: Option<u64>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let ban_expires_at = match ban_duration_secs {
            Some(duration) => now.saturating_add(duration),
            None => 0, // Permanent ban
        };

        // Truncate reason to 256 bytes
        let reason_bytes: Vec<u8> = reason.as_bytes().iter().take(256).copied().collect();

        Self {
            banned_at: now,
            ban_expires_at,
            reason: reason_bytes,
        }
    }

    /// Check if the ban has expired.
    pub fn is_expired(&self) -> bool {
        if self.ban_expires_at == 0 {
            return false; // Permanent ban never expires
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now >= self.ban_expires_at
    }

    /// Get the ban reason as a string.
    pub fn reason_string(&self) -> String {
        String::from_utf8_lossy(&self.reason).to_string()
    }
}

/// Table for storing banned peers.
///
/// Key: peer ID bytes (variable length, typically 38 bytes for secp256k1)
/// Value: BannedPeerEntry (SSZ encoded)
pub struct BannedPeersTable {
    pub db: Arc<Database>,
}

impl BannedPeersTable {
    pub const TABLE_DEFINITION: TableDefinition<'static, &'static [u8], &'static [u8]> =
        TableDefinition::new("banned_peers");

    /// Get a banned peer entry by peer ID bytes.
    pub fn get(&self, peer_id_bytes: &[u8]) -> Result<Option<BannedPeerEntry>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(peer_id_bytes)?;
        Ok(result.map(|v| {
            BannedPeerEntry::from_ssz_bytes(v.value())
                .expect("Failed to decode BannedPeerEntry from SSZ")
        }))
    }

    /// Insert or update a banned peer entry.
    pub fn insert(&self, peer_id_bytes: &[u8], entry: &BannedPeerEntry) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.insert(peer_id_bytes, entry.as_ssz_bytes().as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Remove a banned peer entry.
    pub fn remove(&self, peer_id_bytes: &[u8]) -> Result<Option<BannedPeerEntry>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let value = {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.remove(peer_id_bytes)?.map(|v| {
                BannedPeerEntry::from_ssz_bytes(v.value())
                    .expect("Failed to decode BannedPeerEntry from SSZ")
            })
        };
        write_txn.commit()?;
        Ok(value)
    }

    /// Get all banned peers (peer ID bytes + entry).
    pub fn get_all(&self) -> Result<Vec<(Vec<u8>, BannedPeerEntry)>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        let mut result = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            let entry = BannedPeerEntry::from_ssz_bytes(value.value())
                .expect("Failed to decode BannedPeerEntry from SSZ");
            result.push((key.value().to_vec(), entry));
        }

        Ok(result)
    }

    /// Remove all expired bans and return the count of removed entries.
    pub fn cleanup_expired(&self) -> Result<usize, StoreError> {
        let all_entries = self.get_all()?;
        let mut removed_count = 0;

        for (peer_id_bytes, entry) in all_entries {
            if entry.is_expired() {
                self.remove(&peer_id_bytes)?;
                removed_count += 1;
            }
        }

        Ok(removed_count)
    }

    /// Count the number of banned peers.
    pub fn count(&self) -> Result<usize, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table.len()? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banned_peer_entry_creation() {
        let entry = BannedPeerEntry::new("Test ban reason", Some(3600));

        assert!(!entry.is_expired());
        assert_eq!(entry.reason_string(), "Test ban reason");
        assert!(entry.ban_expires_at > entry.banned_at);
    }

    #[test]
    fn test_banned_peer_entry_permanent() {
        let entry = BannedPeerEntry::new("Permanent ban", None);

        assert!(!entry.is_expired());
        assert_eq!(entry.ban_expires_at, 0);
    }

    #[test]
    fn test_banned_peer_entry_expired() {
        let mut entry = BannedPeerEntry::new("Test", Some(3600));
        // Simulate expired ban by setting expires_at to past
        entry.ban_expires_at = 1;

        assert!(entry.is_expired());
    }

    #[test]
    fn test_banned_peer_entry_ssz_roundtrip() {
        let entry = BannedPeerEntry::new("Test reason", Some(3600));
        let encoded = entry.as_ssz_bytes();
        let decoded = BannedPeerEntry::from_ssz_bytes(&encoded).unwrap();

        assert_eq!(entry, decoded);
    }
}
