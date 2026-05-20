use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A point-in-time snapshot of a successful deploy.
/// Stored in ~/.arkon/snapshots/<project>/<id>.json
/// Does NOT store full file copies — just hashes + metadata for diff-based restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub project: String,
    pub target: String,
    pub adapter: String,
    pub artifact_fingerprint: String,
    /// per-file SHA-256 hashes at time of deploy
    pub file_hashes: HashMap<String, String>,
    pub size_bytes: u64,
    pub deployed_at: DateTime<Utc>,
    pub deploy_record_id: String,
    pub notes: Option<String>,
}

impl Snapshot {
    pub fn label(&self) -> String {
        format!(
            "{}  [{}→{}]  {}",
            self.deployed_at.format("%Y-%m-%d %H:%M UTC"),
            self.adapter,
            self.target,
            &self.id[..8],
        )
    }
}
