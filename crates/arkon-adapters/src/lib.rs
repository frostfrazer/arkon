pub mod fingerprint;
pub mod registry;
pub mod runner;

// built-in adapters
pub mod adapters {
    pub mod android;
    pub mod docker;
    pub mod go;
    pub mod ios;
    pub mod nodejs;
    pub mod nextjs;
    pub mod python;
    pub mod rust_bin;
    pub mod shell;
    pub mod r#static;
    pub mod unity;
    pub mod vite;
}

pub use registry::AdapterRegistry;
pub use runner::BuildRunner;

use arkon_core::{
    artifact::{Artifact, DeployableKind},
    error::Result,
    runtime::{CostHint, Runtime},
    config::TargetConfig,
};
use std::path::{Path, PathBuf};

/// Build context passed to every adapter.
#[derive(Debug, Clone)]
pub struct BuildCtx {
    /// Absolute path to the project root.
    pub root: PathBuf,
    /// Environment variables to inject during build (decrypted from vault).
    pub env: std::collections::HashMap<String, String>,
    /// Override build command from arkon.toml [build] section.
    pub command_override: Option<String>,
    /// Override output directory from arkon.toml [build] section.
    pub output_dir_override: Option<String>,
    /// Whether build caching is enabled.
    pub cache_enabled: bool,
    /// Cached fingerprint of last successful build (empty if none).
    pub last_fingerprint: String,
}

impl BuildCtx {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            env: Default::default(),
            command_override: None,
            output_dir_override: None,
            cache_enabled: true,
            last_fingerprint: String::new(),
        }
    }
}

/// The core adapter trait. Every supported stack implements this.
/// Community adapters loaded from Git repos also implement this via a
/// thin FFI shim or are shelled out via the JSON manifest protocol.
pub trait Adapter: Send + Sync {
    /// Short identifier used in arkon.toml and CLI output. e.g. "nextjs"
    fn name(&self) -> &str;

    /// Human-readable description. e.g. "Next.js application"
    fn description(&self) -> &str;

    /// Perform the build. Returns a fully-populated Artifact on success.
    fn build(&self, ctx: &BuildCtx) -> Result<Artifact>;

    /// Where build artifacts land relative to project root.
    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf;

    /// The runtime environment required on the target.
    fn runtime_info(&self) -> Runtime;

    /// What kind of deployable this adapter produces.
    fn deployable_type(&self) -> DeployableKind;

    /// A stable cache key derived from build inputs.
    /// If this matches the previous key, the build is skipped and cache reused.
    fn cache_key(&self, ctx: &BuildCtx) -> String;

    /// Hook run before the build command. Default: no-op.
    fn pre_build(&self, _ctx: &BuildCtx) -> Result<()> {
        Ok(())
    }

    /// Hook run after a successful build. Default: no-op.
    fn post_build(&self, _artifact: &Artifact) -> Result<()> {
        Ok(())
    }

    /// Optional cost estimate for deploying to a given target.
    fn cost_estimate(&self, _artifact: &Artifact, _target: &TargetConfig) -> Option<CostHint> {
        None
    }
}
