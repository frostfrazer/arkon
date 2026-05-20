use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// What kind of deployable unit an adapter produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployableKind {
    /// Static files — HTML, JS, CSS, assets.
    Static,
    /// A Docker image (tarball or image ID).
    Container,
    /// A compiled native binary.
    Binary,
    /// A game build directory (platform-specific).
    GameBuild,
    /// WebAssembly module + JS glue.
    Wasm,
}

/// The output of a successful adapter build.
/// Passed directly to the Deployment Dispatcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Human-readable artifact name, e.g. "my-app@0.1.0-linux"
    pub name: String,
    /// Absolute path to the build output directory or file.
    pub path: PathBuf,
    /// What kind of deployable this is.
    pub kind: DeployableKind,
    /// SHA-256 fingerprint of the entire output tree (hex string).
    /// Used by the diff engine to skip unchanged files.
    pub fingerprint: String,
    /// Per-file fingerprints: relative path → SHA-256 hex.
    pub file_hashes: HashMap<String, String>,
    /// Total size of all output files, in bytes.
    pub size_bytes: u64,
    /// Metadata the adapter wants to pass to targets.
    pub meta: HashMap<String, String>,
}

impl Artifact {
    pub fn new(
        name: impl Into<String>,
        path: PathBuf,
        kind: DeployableKind,
    ) -> Self {
        Self {
            name: name.into(),
            path,
            kind,
            fingerprint: String::new(),
            file_hashes: HashMap::new(),
            size_bytes: 0,
            meta: HashMap::new(),
        }
    }

    /// Returns true if this artifact's fingerprint matches a previous one,
    /// meaning nothing changed and we can skip re-deploying.
    pub fn unchanged_from(&self, previous_fingerprint: &str) -> bool {
        !self.fingerprint.is_empty() && self.fingerprint == previous_fingerprint
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }
}
