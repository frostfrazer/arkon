use crate::{
    Adapter, BuildCtx,
    adapters::{
        android::AndroidAdapter,
        docker::DockerAdapter,
        go::GoAdapter,
        ios::IosAdapter,
        nextjs::NextjsAdapter,
        nodejs::NodejsAdapter,
        python::PythonAdapter,
        rust_bin::RustBinAdapter,
        shell::ShellAdapter,
        r#static::StaticAdapter,
        unity::UnityAdapter,
        vite::ViteAdapter,
    },
};
use arkon_core::error::{ArkonError, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

type BoxedAdapter = Arc<dyn Adapter>;

/// Central registry of all adapters — built-in and community-loaded.
/// Thread-safe; supports hot-reload of community adapters at runtime.
pub struct AdapterRegistry {
    inner: RwLock<HashMap<String, BoxedAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl AdapterRegistry {
    /// Creates a registry pre-populated with all built-in adapters.
    pub fn with_builtins() -> Self {
        let mut map: HashMap<String, BoxedAdapter> = HashMap::new();

        let builtins: Vec<BoxedAdapter> = vec![
            Arc::new(NextjsAdapter),
            Arc::new(ViteAdapter),
            Arc::new(NodejsAdapter),
            Arc::new(PythonAdapter),
            Arc::new(GoAdapter),
            Arc::new(RustBinAdapter),
            Arc::new(DockerAdapter),
            Arc::new(UnityAdapter),
            Arc::new(StaticAdapter),
            Arc::new(ShellAdapter),
            Arc::new(AndroidAdapter),
            Arc::new(IosAdapter),
        ];

        for adapter in builtins {
            info!(adapter = %adapter.name(), "registered built-in adapter");
            map.insert(adapter.name().to_string(), adapter);
        }

        Self {
            inner: RwLock::new(map),
        }
    }

    /// Retrieve an adapter by name.
    pub fn get(&self, name: &str) -> Result<Arc<dyn Adapter>> {
        self.inner
            .read()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| ArkonError::AdapterNotFound { name: name.to_string() })
    }

    /// Register or replace an adapter. Used by community adapter loader and hot-reload.
    pub fn register(&self, adapter: impl Adapter + 'static) {
        let name = adapter.name().to_string();
        info!(adapter = %name, "registering adapter");
        self.inner.write().unwrap().insert(name, Arc::new(adapter));
    }

    /// List all registered adapter names.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.inner.read().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    /// Load community adapters from a directory of JSON manifests.
    /// Each manifest describes a shell-based adapter (name, build command, output_dir, etc.).
    /// Hot-reload: calling this again replaces only the adapters whose manifests changed.
    pub fn load_community_dir(&self, dir: &std::path::Path) -> Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }
        let mut loaded = 0;
        for entry in std::fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match ShellManifestAdapter::load(&path) {
                    Ok(adapter) => {
                        info!(adapter = %adapter.name(), path = %path.display(), "loaded community adapter");
                        self.register(adapter);
                        loaded += 1;
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "failed to load community adapter");
                    }
                }
            }
        }
        Ok(loaded)
    }
}

// ─── Shell-manifest community adapter ────────────────────────────────────────

use arkon_core::{artifact::{Artifact, DeployableKind}, runtime::Runtime, config::TargetConfig, runtime::CostHint};
use serde::Deserialize;

/// JSON manifest format for community shell-based adapters.
#[derive(Debug, Deserialize)]
struct ShellManifest {
    name: String,
    description: String,
    build_command: String,
    output_dir: String,
    deployable_type: String,
    cache_inputs: Vec<String>,
}

struct ShellManifestAdapter {
    manifest: ShellManifest,
}

impl ShellManifestAdapter {
    fn load(path: &std::path::Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let manifest: ShellManifest = serde_json::from_str(&raw)
            .map_err(|e| ArkonError::ConfigError(e.to_string()))?;
        Ok(Self { manifest })
    }
}

impl Adapter for ShellManifestAdapter {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn description(&self) -> &str {
        &self.manifest.description
    }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        crate::runner::run_shell_command(&self.manifest.build_command, &ctx.root, &ctx.env)?;
        let output = ctx.root.join(&self.manifest.output_dir);
        let (fingerprint, file_hashes, size_bytes) =
            crate::fingerprint::fingerprint_dir(&output)?;
        let mut artifact = Artifact::new(&self.manifest.name, output, self.deployable_type());
        artifact.fingerprint = fingerprint;
        artifact.file_hashes = file_hashes;
        artifact.size_bytes = size_bytes;
        Ok(artifact)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> std::path::PathBuf {
        ctx.root.join(&self.manifest.output_dir)
    }

    fn runtime_info(&self) -> Runtime {
        Runtime::native()
    }

    fn deployable_type(&self) -> DeployableKind {
        match self.manifest.deployable_type.as_str() {
            "static" => DeployableKind::Static,
            "container" => DeployableKind::Container,
            "game_build" => DeployableKind::GameBuild,
            "wasm" => DeployableKind::Wasm,
            _ => DeployableKind::Binary,
        }
    }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        let refs: Vec<&str> = self.manifest.cache_inputs.iter().map(|s| s.as_str()).collect();
        crate::fingerprint::cache_key_from_files(&ctx.root, &refs)
    }
}
