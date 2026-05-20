use serde::{Deserialize, Serialize};

/// The runtime environment an adapter requires on the target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    pub kind: RuntimeKind,
    /// Minimum required version, e.g. "20.0.0"
    pub version: Option<String>,
    /// Environment variables to inject at runtime (keys only — values come from vault).
    pub env_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Node,
    Python,
    Rust,
    Go,
    Deno,
    Bun,
    Docker,
    Static, // no runtime — just a web server
    Native, // self-contained binary
}

impl Runtime {
    pub fn static_files() -> Self {
        Self {
            kind: RuntimeKind::Static,
            version: None,
            env_keys: vec![],
        }
    }

    pub fn node(version: impl Into<String>) -> Self {
        Self {
            kind: RuntimeKind::Node,
            version: Some(version.into()),
            env_keys: vec![],
        }
    }

    pub fn native() -> Self {
        Self {
            kind: RuntimeKind::Native,
            version: None,
            env_keys: vec![],
        }
    }

    pub fn with_env(mut self, keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.env_keys = keys.into_iter().map(Into::into).collect();
        self
    }
}

/// Estimated cost of a deploy operation, before it runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostHint {
    /// Estimated upload cost in USD.
    pub upload_usd: f64,
    /// Estimated monthly egress cost in USD.
    pub egress_monthly_usd: f64,
    /// Human-readable breakdown.
    pub breakdown: String,
}

impl CostHint {
    pub fn free() -> Self {
        Self {
            upload_usd: 0.0,
            egress_monthly_usd: 0.0,
            breakdown: "Free tier — no cost".into(),
        }
    }

    pub fn total(&self) -> f64 {
        self.upload_usd + self.egress_monthly_usd
    }
}
