use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{ArkonError, Result};

/// Root structure of arkon.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkonConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub deploy: DeployConfig,
    #[serde(default)]
    pub targets: HashMap<String, TargetConfig>,
    #[serde(default)]
    pub hooks: HashMap<String, HooksConfig>,
    #[serde(default)]
    pub health: HashMap<String, HealthConfig>,
    #[serde(default)]
    pub adapters: AdapterRegistryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    /// Force a specific adapter. Auto-detected if absent.
    pub adapter: Option<String>,
    pub node: Option<String>,
    #[serde(default = "default_true")]
    pub cache: bool,
    #[serde(default)]
    pub wasm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildConfig {
    pub command: Option<String>,
    pub output_dir: Option<String>,
    pub env_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeployConfig {
    pub default_target: Option<String>,
    #[serde(default)]
    pub confirm_cost: bool,
    /// Prompt before deploying if cost estimate exceeds this USD amount.
    pub cost_threshold_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TargetConfig {
    Ssh(SshTarget),
    S3(S3Target),
    B2(B2Target),
    R2(R2Target),
    Webrtc(WebrtcTarget),
    Ipfs(IpfsTarget),
    GithubPages(GithubPagesTarget),
    Local(LocalTarget),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTarget {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    #[serde(default)]
    pub tls: bool,
    pub domain: Option<String>,
    pub identity: Option<PathBuf>,
    /// Accept unknown host keys on first connection (still rejects changed keys).
    /// Default false — add new hosts with ssh-keyscan first.
    #[serde(default)]
    pub accept_new_host: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Target {
    pub bucket: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    #[serde(default)]
    pub invalidate: bool,
    pub cloudfront_distribution_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct B2Target {
    pub bucket: String,
    #[serde(default)]
    pub torrent: bool,
    #[serde(default)]
    pub dl_page: bool,
    pub dl_page_template: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R2Target {
    pub bucket: String,
    pub account_id: String,
    #[serde(default)]
    pub invalidate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebrtcTarget {
    pub ttl: Option<String>,
    pub stun: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsTarget {
    pub api: Option<String>,
    pub pin_service: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPagesTarget {
    pub repo: String,
    pub branch: Option<String>,
    pub cname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTarget {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre_deploy: Vec<String>,
    #[serde(default)]
    pub post_deploy: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthConfig {
    #[serde(default)]
    pub checks: Vec<HealthCheck>,
    pub interval: Option<String>,
    pub retries: Option<u32>,
    pub on_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HealthCheck {
    Http { url: String, expect: Option<u16> },
    Tcp { host: String, port: u16 },
    Process { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterRegistryConfig {
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default = "default_true")]
    pub hot_reload: bool,
}

fn default_true() -> bool { true }

impl ArkonConfig {
    /// Load arkon.toml, walking up from `start` until found.
    pub fn load(start: &Path) -> Result<(Self, PathBuf)> {
        let mut dir = start.to_path_buf();
        loop {
            let candidate = dir.join("arkon.toml");
            if candidate.exists() {
                let raw = std::fs::read_to_string(&candidate)
                    .map_err(|e| ArkonError::ConfigError(e.to_string()))?;
                let cfg: ArkonConfig = toml::from_str(&raw)
                    .map_err(|e| ArkonError::ConfigError(e.to_string()))?;
                return Ok((cfg, candidate));
            }
            if !dir.pop() {
                break;
            }
        }
        Err(ArkonError::ConfigError(
            "arkon.toml not found. Run `arkon init` to create one.".into(),
        ))
    }

    /// Returns the TargetConfig for a given name.
    pub fn target(&self, name: &str) -> Result<&TargetConfig> {
        self.targets.get(name).ok_or_else(|| ArkonError::TargetNotFound {
            name: name.to_string(),
        })
    }
}
