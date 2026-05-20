pub mod dispatcher;
pub mod targets {
    pub mod ssh;
    pub mod s3;
    pub mod local;
    pub mod docker;
    pub mod ipfs;
    pub mod github_pages;
    pub mod torrent;
    pub mod webrtc;
}

pub use dispatcher::Dispatcher;

use arkon_core::{
    artifact::Artifact,
    config::TargetConfig,
    deploy::DeployCtx,
    error::Result,
    runtime::CostHint,
};

/// Every deployment target implements this trait.
pub trait Target: Send + Sync {
    /// Short identifier matching arkon.toml `type` field. e.g. "ssh", "s3"
    fn name(&self) -> &str;

    /// Push the artifact to this target.
    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl>;

    /// Estimate cost before deploying. None = free or unknown.
    fn cost_estimate(&self, artifact: &Artifact, config: &TargetConfig) -> Option<CostHint> {
        None
    }

    /// Verify the target is reachable before deploying. Called pre-flight.
    fn health_check(&self, config: &TargetConfig) -> Result<()> {
        Ok(())
    }
}

/// The public URL / connection string of a successful deploy.
#[derive(Debug, Clone)]
pub struct DeployedUrl {
    pub url: String,
    pub kind: DeployedUrlKind,
}

#[derive(Debug, Clone)]
pub enum DeployedUrlKind {
    Http,
    P2p,
    Ipfs,
    Local,
    Torrent { magnet: String },
}

impl DeployedUrl {
    pub fn http(url: impl Into<String>) -> Self {
        Self { url: url.into(), kind: DeployedUrlKind::Http }
    }
    pub fn local(path: impl Into<String>) -> Self {
        Self { url: path.into(), kind: DeployedUrlKind::Local }
    }
}
