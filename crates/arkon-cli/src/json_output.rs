//! Machine-readable JSON output mode.
//!
//! When `--json` is passed, ARKON suppresses all interactive prompts and
//! coloured terminal output, and instead writes a single JSON object to stdout
//! on completion. This enables clean integration with CI/CD pipelines, scripts,
//! and other tooling.
//!
//! Schema is stable across minor versions (breaking changes only on major).
//!
//! Example (success):
//! ```json
//! {
//!   "ok": true,
//!   "version": "0.1.0",
//!   "command": "ship",
//!   "project": "my-app",
//!   "adapter": "nextjs",
//!   "target": "production",
//!   "url": "https://my-bucket.s3.eu-central-1.amazonaws.com",
//!   "snapshot_id": "a1b2c3d4",
//!   "artifact_fingerprint": "deadbeef...",
//!   "size_bytes": 4194304,
//!   "duration_ms": 12430,
//!   "deployed_at": "2024-11-03T14:22:00Z"
//! }
//! ```
//!
//! Example (error):
//! ```json
//! {
//!   "ok": false,
//!   "version": "0.1.0",
//!   "command": "ship",
//!   "error": "deploy failed to target 'production': connection refused"
//! }
//! ```

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

static JSON_MODE: AtomicBool = AtomicBool::new(false);

/// Enable JSON output mode. Called from main() when --json is set.
pub fn enable() {
    JSON_MODE.store(true, Ordering::Relaxed);
}

/// Returns true if JSON output mode is active.
pub fn is_enabled() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

/// Print a success result as JSON and exit 0.
pub fn output_success(result: &JsonResult) {
    println!("{}", serde_json::to_string_pretty(result).unwrap_or_default());
}

/// Print an error as JSON and exit 1.
pub fn output_error(command: &str, error: &str) {
    let v = serde_json::json!({
        "ok":      false,
        "version": env!("CARGO_PKG_VERSION"),
        "command": command,
        "error":   error,
    });
    eprintln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

/// The JSON schema for a successful operation result.
#[derive(Debug, Serialize, Default)]
pub struct JsonResult {
    pub ok:                   bool,
    pub version:              String,
    pub command:              String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project:              String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub adapter:              String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target:               String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url:                  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id:          Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub artifact_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes:           Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms:          Option<u64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub deployed_at:          String,
    // For detect command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence:           Option<f32>,
    // For rollback/promote
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_target:          Option<String>,
    // For preview
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id:              Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_secs:             Option<u64>,
}

impl JsonResult {
    pub fn new(command: &str) -> Self {
        Self {
            ok:      true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            command: command.to_string(),
            ..Default::default()
        }
    }
}
