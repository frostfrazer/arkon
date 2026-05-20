use crate::{
    identity::PeerIdentity,
    relay::{self, RelayHandle},
    server::LocalServer,
};
use arkon_core::error::{ArkonError, Result};
use std::path::PathBuf;
use tokio::time::{self, Duration};
use tracing::info;

pub struct PreviewConfig {
    pub artifact_path: PathBuf,
    pub project_name: String,
    /// TTL in seconds before the preview auto-expires.
    pub ttl: u64,
    pub relay_url: String,
    pub local_port: u16,
}

/// A running P2P preview session. Drop to shut down immediately,
/// or call `wait_for_expiry()` to block until TTL elapses.
pub struct PreviewSession {
    /// The URL visitors open in their browser.
    pub public_url: String,
    /// The local HTTP URL (for debugging).
    pub local_url: String,
    pub peer_id: String,
    pub ttl_secs: u64,
    server: LocalServer,
    relay: RelayHandle,
}

impl PreviewSession {
    pub async fn start(config: PreviewConfig) -> Result<Self> {
        // 1. Load or generate peer identity
        let identity = PeerIdentity::load_or_create(&config.project_name)?;

        // 2. Start local HTTP server
        let server = LocalServer::start(&config.artifact_path, config.local_port).await?;
        let local_url = server.url();

        // 3. Register with relay
        let token = relay::generate_token();
        let (relay_handle, public_url) = RelayHandle::register(
            &config.relay_url,
            &identity.peer_id,
            server.addr,
            config.ttl,
            &token,
        )
        .await?;

        info!(
            peer_id   = %identity.peer_id,
            local_url = %local_url,
            public_url = %public_url,
            ttl_secs  = %config.ttl,
            "preview session started"
        );

        Ok(Self {
            public_url,
            local_url,
            peer_id: identity.peer_id,
            ttl_secs: config.ttl,
            server,
            relay: relay_handle,
        })
    }

    /// Block until TTL elapses, then shut down the session.
    pub async fn wait_for_expiry(self) {
        let ttl = self.ttl_secs;
        info!(ttl_secs = %ttl, "waiting for preview TTL...");
        time::sleep(Duration::from_secs(ttl)).await;
        self.shutdown().await;
    }

    /// Immediately shut down the server and deregister from the relay.
    pub async fn shutdown(self) {
        self.relay.deregister().await;
        self.server.stop();
        info!("preview session ended");
    }

    /// Formatted TTL string for display (e.g. "24h", "45m").
    pub fn ttl_display(&self) -> String {
        let secs = self.ttl_secs;
        if secs >= 3600 && secs % 3600 == 0 {
            format!("{}h", secs / 3600)
        } else if secs >= 60 {
            format!("{}m", secs / 60)
        } else {
            format!("{}s", secs)
        }
    }
}
