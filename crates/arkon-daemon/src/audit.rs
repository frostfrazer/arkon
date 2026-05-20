use arkon_core::{deploy::DeployRecord, error::Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Append-only, HMAC-chained audit log.
/// Each entry contains the HMAC of (previous_entry_id + this_entry_json)
/// making it tamper-evident: any modification breaks the chain.
pub struct AuditLog {
    path:    PathBuf,
    last_id: String,
}

impl AuditLog {
    /// Open the global audit log at `~/.arkon/audit.log`.
    pub fn open(project: &str) -> Result<Self> {
        let path = audit_log_path();
        Self::new_at(path)
    }

    /// Create an audit log at an arbitrary path (used in tests).
    pub fn new(path: PathBuf) -> Self {
        let last_id = if path.exists() {
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .last()
                .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .and_then(|v| v["id"].as_str().map(String::from))
                .unwrap_or_default()
        } else {
            String::new()
        };
        Self { path, last_id }
    }

    fn new_at(path: PathBuf) -> Result<Self> {
        let last_id = if path.exists() {
            std::fs::read_to_string(&path)?
                .lines()
                .last()
                .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .and_then(|v| v["id"].as_str().map(String::from))
                .unwrap_or_default()
        } else {
            String::new()
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self { path, last_id })
    }

    /// Path to the log file (used in tests to tamper with contents).
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Read all records from the log without verifying HMAC.
    pub fn read_all(&self) -> Result<Vec<DeployRecord>> {
        if !self.path.exists() { return Ok(vec![]); }
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(raw.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<DeployRecord>(l).ok())
            .collect())
    }

    /// Read all records AND verify the full HMAC chain.
    /// Returns `(records, chain_valid)`.
    pub fn read_and_verify(&self) -> Result<(Vec<DeployRecord>, bool)> {
        if !self.path.exists() { return Ok((vec![], true)); }
        let raw     = std::fs::read_to_string(&self.path)?;
        let mut prev_id     = String::new();
        let mut chain_valid = true;
        let mut records     = Vec::new();

        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let record: DeployRecord = match serde_json::from_str(line) {
                Ok(r)  => r,
                Err(e) => {
                    tracing::warn!(error = %e, "unparseable audit log line");
                    chain_valid = false;
                    continue;
                }
            };

            // Recompute expected HMAC
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(line) {
                v["chain_hmac"] = serde_json::Value::String(String::new());
                if let Ok(without_hmac) = serde_json::to_string(&v) {
                    let mut h = Sha256::new();
                    h.update(prev_id.as_bytes());
                    h.update(without_hmac.as_bytes());
                    let expected = hex::encode(h.finalize());
                    if record.chain_hmac != expected {
                        tracing::error!(id = %record.id, "audit chain BROKEN");
                        chain_valid = false;
                    }
                }
            }

            prev_id = record.id.clone();
            records.push(record);
        }
        Ok((records, chain_valid))
    }

    /// Append a deploy record to the log. Computes and sets the chain HMAC.
    pub fn append(&mut self, mut record: DeployRecord) -> Result<()> {
        // Compute HMAC: SHA-256(prev_id || record_json_without_hmac)
        let record_json = serde_json::to_string(&record)
            .map_err(|e| arkon_core::error::ArkonError::Other(e.into()))?;
        let mut h = Sha256::new();
        h.update(self.last_id.as_bytes());
        h.update(record_json.as_bytes());
        record.chain_hmac = hex::encode(h.finalize());

        let final_json = serde_json::to_string(&record)
            .map_err(|e| arkon_core::error::ArkonError::Other(e.into()))?;

        // Append to log file (one JSON object per line — NDJSON)
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{final_json}")?;

        self.last_id = record.id;
        tracing::debug!(id = %self.last_id, "audit log entry written");
        Ok(())
    }

    /// Verify the entire chain integrity. Returns the number of valid entries.
    pub fn verify(&self) -> Result<(usize, bool)> {
        let raw = std::fs::read_to_string(&self.path)?;
        let mut prev_id = String::new();
        let mut count = 0;

        for line in raw.lines() {
            let record: DeployRecord = serde_json::from_str(line)
                .map_err(|e| arkon_core::error::ArkonError::Other(e.into()))?;

            // Recompute expected HMAC
            let without_hmac = {
                let mut v: serde_json::Value = serde_json::from_str(line)
                    .map_err(|e| arkon_core::error::ArkonError::Other(e.into()))?;
                v["chain_hmac"] = serde_json::Value::String(String::new());
                serde_json::to_string(&v).unwrap()
            };
            let mut h = Sha256::new();
            h.update(prev_id.as_bytes());
            h.update(without_hmac.as_bytes());
            let expected = hex::encode(h.finalize());

            if record.chain_hmac != expected {
                tracing::error!(id = %record.id, "audit log chain BROKEN at entry");
                return Ok((count, false));
            }
            prev_id = record.id;
            count += 1;
        }
        Ok((count, true))
    }
}

fn audit_log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".arkon")
        .join("audit.log")
}
