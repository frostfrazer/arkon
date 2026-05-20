//! ARKON P2P Preview
//!
//! Sprint 2 implementation: HTTP-based reverse proxy over a lightweight signalling
//! relay. Full libp2p/WebRTC integration is planned for sprint 3; this sprint
//! delivers a working `arkon preview` via a minimal TURN-free signalling approach:
//!
//!  1. Local HTTP server starts on a random port serving the built artifact.
//!  2. A peer identity (stable per-project Ed25519 keypair) is generated/loaded.
//!  3. ARKON registers the local address with a free public signalling relay
//!     (self-hostable; source in `relay/` directory) using a short-lived token.
//!  4. Visitors resolve `arkon://peer-id` → relay → TCP proxy → local server.
//!  5. After TTL expires, the registration is removed and the server shuts down.
//!
//! Zero bandwidth cost to ARKON — the relay only forwards connection metadata,
//! not data. Data flows peer-to-peer via TCP hole-punching where possible, or
//! via the visitor's own network path.

pub mod dht;
pub mod identity;
pub mod matchmaking;
pub mod preview;
pub mod relay;
pub mod server;

pub use dht::{DhtNode, DhtProvider};
pub use matchmaking::{LanBroadcaster, LanScanner, InternetMatchmaker};
pub use preview::{PreviewSession, PreviewConfig};

use arkon_core::error::Result;
use std::path::Path;

/// Start a P2P preview for the artifact at `artifact_path`.
/// Returns a `PreviewSession` with the shareable link and a shutdown handle.
pub async fn start_preview(
    artifact_path: &Path,
    project_name: &str,
    ttl: &str,
) -> Result<PreviewSession> {
    let config = PreviewConfig {
        artifact_path: artifact_path.to_path_buf(),
        project_name: project_name.to_string(),
        ttl: parse_ttl(ttl),
        relay_url: default_relay_url(),
        local_port: 0, // OS assigns a free port
    };
    PreviewSession::start(config).await
}

/// Parse a TTL string like "24h", "30m", "3600s" into seconds.
pub fn parse_ttl(s: &str) -> u64 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix('h') { return v.parse::<u64>().unwrap_or(24) * 3600; }
    if let Some(v) = s.strip_suffix('m') { return v.parse::<u64>().unwrap_or(60) * 60; }
    if let Some(v) = s.strip_suffix('s') { return v.parse().unwrap_or(3600); }
    3600 * 24 // default 24h
}

fn default_relay_url() -> String {
    std::env::var("ARKON_RELAY_URL")
        .unwrap_or_else(|_| "https://relay.arkon.sh".into())
}
