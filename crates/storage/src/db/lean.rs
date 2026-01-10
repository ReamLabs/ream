use std::{fmt::Write, sync::Arc};

use redb::{Database, ReadableDatabase, ReadableTableMetadata};
use tracing::info;

use crate::{
    cache::LeanCacheDB,
    tables::{
        banned_peers::BannedPeersTable,
        field::REDBField,
        lean::{
            latest_finalized::LatestFinalizedField, latest_justified::LatestJustifiedField,
            latest_known_attestation::LatestKnownAttestationTable, lean_block::LeanBlockTable,
            lean_head::LeanHeadField, lean_latest_new_attestations::LeanLatestNewAttestationsTable,
            lean_safe_target::LeanSafeTargetField, lean_state::LeanStateTable,
            lean_time::LeanTimeField, slot_index::LeanSlotIndexTable,
            state_root_index::LeanStateRootIndexTable,
        },
        table::REDBTable,
    },
};

#[derive(Clone, Debug)]
pub struct LeanDB {
    pub db: Arc<Database>,
    pub(crate) cache: Option<Arc<LeanCacheDB>>,
}

impl LeanDB {
    /// Attach a cache to this LeanDB instance.
    /// This enables in-memory caching of blocks and states for improved performance.
    pub fn with_cache(mut self, cache: Arc<LeanCacheDB>) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn block_provider(&self) -> LeanBlockTable {
        LeanBlockTable {
            db: self.db.clone(),
            cache: self.cache.clone(),
        }
    }
    pub fn state_provider(&self) -> LeanStateTable {
        LeanStateTable {
            db: self.db.clone(),
            cache: self.cache.clone(),
        }
    }

    pub fn slot_index_provider(&self) -> LeanSlotIndexTable {
        LeanSlotIndexTable {
            db: self.db.clone(),
        }
    }

    pub fn state_root_index_provider(&self) -> LeanStateRootIndexTable {
        LeanStateRootIndexTable {
            db: self.db.clone(),
        }
    }

    pub fn latest_known_attestations_provider(&self) -> LatestKnownAttestationTable {
        LatestKnownAttestationTable {
            db: self.db.clone(),
        }
    }

    pub fn latest_finalized_provider(&self) -> LatestFinalizedField {
        LatestFinalizedField {
            db: self.db.clone(),
        }
    }

    pub fn latest_justified_provider(&self) -> LatestJustifiedField {
        LatestJustifiedField {
            db: self.db.clone(),
        }
    }

    pub fn time_provider(&self) -> LeanTimeField {
        LeanTimeField {
            db: self.db.clone(),
        }
    }

    pub fn head_provider(&self) -> LeanHeadField {
        LeanHeadField {
            db: self.db.clone(),
        }
    }

    pub fn safe_target_provider(&self) -> LeanSafeTargetField {
        LeanSafeTargetField {
            db: self.db.clone(),
        }
    }

    pub fn latest_new_attestations_provider(&self) -> LeanLatestNewAttestationsTable {
        LeanLatestNewAttestationsTable {
            db: self.db.clone(),
        }
    }

    /// Checks the storage usage of all tables and reports metrics.
    ///
    /// # Arguments
    ///
    /// * `threshold_mb` - Only report if total storage exceeds this many megabytes. Pass `0` to
    ///   force print the report.
    pub fn report_storage_metrics(&self, threshold_mb: u64) -> anyhow::Result<()> {
        let threshold_bytes = threshold_mb * 1024 * 1024;
        let read_txn = self.db.begin_read()?;
        let mut total_bytes = 0u64;
        let mut table_metrics = Vec::new();

        macro_rules! collect_stats {
            ($table_type:ty, $name:expr) => {
                let table = read_txn.open_table(<$table_type>::TABLE_DEFINITION)?;
                let stats = table.stats()?;
                let size = stats.stored_bytes() + stats.metadata_bytes() + stats.fragmented_bytes();
                total_bytes += size;
                table_metrics.push(($name, size));
            };
        }

        macro_rules! collect_field_stats {
            ($field_type:ty, $name:expr) => {
                let table = read_txn.open_table(<$field_type>::FIELD_DEFINITION)?;
                let stats = table.stats()?;
                let size = stats.stored_bytes() + stats.metadata_bytes() + stats.fragmented_bytes();
                total_bytes += size;
                table_metrics.push(($name, size));
            };
        }

        collect_stats!(LeanBlockTable, "LeanBlockTable");
        collect_stats!(LeanStateTable, "LeanStateTable");
        collect_stats!(LeanSlotIndexTable, "LeanSlotIndexTable");
        collect_stats!(LeanStateRootIndexTable, "LeanStateRootIndexTable");
        collect_stats!(LatestKnownAttestationTable, "LatestKnownAttestationTable");
        collect_stats!(
            LeanLatestNewAttestationsTable,
            "LeanLatestNewAttestationsTable"
        );
        collect_field_stats!(LatestFinalizedField, "LatestFinalizedField");
        collect_field_stats!(LatestJustifiedField, "LatestJustifiedField");
        collect_field_stats!(LeanTimeField, "LeanTimeField");
        collect_field_stats!(LeanHeadField, "LeanHeadField");
        collect_field_stats!(LeanSafeTargetField, "LeanSafeTargetField");

        if total_bytes < threshold_bytes {
            return Ok(());
        }

        table_metrics.sort_by(|a, b| b.1.cmp(&a.1));
        let mut report = String::with_capacity(512);
        let total_mb = total_bytes as f64 / (1024.0 * 1024.0);
        if total_mb >= 1024.0 {
            writeln!(
                report,
                "LeanDB Storage Report (Total: {:.2} GB)",
                total_mb / 1024.0
            )?;
        } else {
            writeln!(report, "LeanDB Storage Report (Total: {total_mb:.2} MB)")?;
        }

        writeln!(report, "{:<35} | {:<15}", "Table Name", "Size")?;
        writeln!(report, "{:-<35}-|-{:-<15}", "", "")?;

        for (name, bytes) in table_metrics {
            if bytes > 0 {
                let kb = bytes as f64 / 1024.0;
                let size_str = if kb < 1024.0 {
                    format!("{kb:.2} KB")
                } else {
                    format!("{:.2} MB", kb / 1024.0)
                };
                writeln!(report, "{name:<35} | {size_str:<15}")?;
            }
        }

        info!("\n{}", report);

        Ok(())
    }

    pub fn banned_peers_provider(&self) -> BannedPeersTable {
        BannedPeersTable {
            db: self.db.clone(),
        }
    }
}
