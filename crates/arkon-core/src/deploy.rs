use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Passed to every target during a deploy operation.
#[derive(Debug, Clone)]
pub struct DeployCtx {
    pub project_name: String,
    pub target_name: String,
    pub project_root: PathBuf,
    pub dry_run: bool,
    /// Injected secrets (already decrypted from vault).
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    Success,
    Failed,
    RolledBack,
    Skipped, // unchanged artifact, nothing to do
}

/// Immutable record written to the audit log after every deploy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRecord {
    pub id: String,
    pub project: String,
    pub target: String,
    pub adapter: String,
    pub artifact_fingerprint: String,
    pub status: DeployStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub size_bytes: u64,
    /// HMAC of previous record ID + this record's content — chain integrity.
    pub chain_hmac: String,
    pub notes: Option<String>,
}

impl DeployRecord {
    pub fn new(
        project: impl Into<String>,
        target: impl Into<String>,
        adapter: impl Into<String>,
        artifact_fingerprint: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            project: project.into(),
            target: target.into(),
            adapter: adapter.into(),
            artifact_fingerprint: artifact_fingerprint.into(),
            status: DeployStatus::Success,
            started_at: now,
            finished_at: now,
            duration_ms: 0,
            size_bytes: 0,
            chain_hmac: String::new(),
            notes: None,
        }
    }
}
