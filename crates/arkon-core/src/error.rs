use thiserror::Error;

pub type Result<T> = std::result::Result<T, ArkonError>;

#[derive(Debug, Error)]
pub enum ArkonError {
    #[error("detection failed: {0}")]
    DetectionFailed(String),

    #[error("build failed: {0}")]
    BuildFailed(String),

    #[error("deploy failed to target '{target}': {reason}")]
    DeployFailed { target: String, reason: String },

    #[error("adapter '{name}' not found")]
    AdapterNotFound { name: String },

    #[error("target '{name}' not found in config")]
    TargetNotFound { name: String },

    #[error("secrets vault error: {0}")]
    VaultError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("hook '{hook}' failed with exit code {code}")]
    HookFailed { hook: String, code: i32 },

    #[error("cost threshold exceeded: estimated ${estimate:.4}, limit ${limit:.4}")]
    CostThresholdExceeded { estimate: f64, limit: f64 },

    #[error("rollback target not found: {snapshot_id}")]
    SnapshotNotFound { snapshot_id: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("network error: {0}")]
    Network(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl ArkonError {
    pub fn deploy(target: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::DeployFailed {
            target: target.into(),
            reason: reason.into(),
        }
    }
}
