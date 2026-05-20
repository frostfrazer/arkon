use arkon_core::{error::{ArkonError, Result}, snapshot::Snapshot};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tracing::debug;

/// Lightweight entry stored in the index (not full snapshot data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub target: String,
    pub adapter: String,
    pub deployed_at: DateTime<Utc>,
    pub artifact_fingerprint: String,
    pub size_bytes: u64,
    /// Relative filename within the project snapshot dir, e.g. "2024-11-03T14-22-00Z_prod_a1b2c3d4.json"
    pub filename: String,
}

/// On-disk index file format.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SnapshotIndex {
    pub project: String,
    pub entries: Vec<IndexEntry>,
}

impl SnapshotIndex {
    pub fn load(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(SnapshotIndex::default());
        }
        let raw = std::fs::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(|e| ArkonError::Other(e.into()))
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| ArkonError::Other(e.into()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn add(&mut self, entry: IndexEntry) {
        // Remove any existing entry with same ID (idempotent)
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
        // Keep sorted newest-first
        self.entries.sort_by(|a, b| b.deployed_at.cmp(&a.deployed_at));
    }

    /// Find a snapshot by fuzzy query:
    ///   - "latest" / "" / None  → most recent
    ///   - 8+ hex chars          → match snapshot ID prefix
    ///   - date-like string      → match by deployed_at prefix (e.g. "2024-11-03", "2024-11-03T14")
    ///   - target name           → most recent for that target
    pub fn find(&self, query: &str, target_filter: Option<&str>) -> Option<&IndexEntry> {
        let entries: Vec<&IndexEntry> = if let Some(t) = target_filter {
            self.entries.iter().filter(|e| e.target == t).collect()
        } else {
            self.entries.iter().collect()
        };

        match query.trim() {
            "" | "latest" => entries.first().copied(),
            q if q.len() >= 6 && q.chars().all(|c| c.is_ascii_hexdigit()) => {
                // ID prefix match
                entries.into_iter().find(|e| e.id.starts_with(q))
            }
            q => {
                // Date/time prefix match against ISO8601 string
                entries.into_iter().find(|e| {
                    let ts = e.deployed_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
                    ts.starts_with(q) || e.deployed_at.format("%Y-%m-%d").to_string().starts_with(q)
                })
            }
        }
    }

    /// Returns all snapshots, newest first, optionally filtered by target.
    pub fn list(&self, target_filter: Option<&str>) -> Vec<&IndexEntry> {
        self.entries
            .iter()
            .filter(|e| target_filter.map(|t| e.target == t).unwrap_or(true))
            .collect()
    }
}
