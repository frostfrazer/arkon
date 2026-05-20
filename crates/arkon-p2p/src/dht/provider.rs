//! DHT-based preview provider — replaces the relay-dependent `PreviewSession`.
//!
//! Workflow:
//!   1. Start a `DhtNode` (libp2p swarm).
//!   2. Start the local HTTP file server.
//!   3. Announce our multiaddr+peer_id as providers for the project DHT key.
//!   4. Visitors run `arkon connect <project>` which:
//!      - Spins up their own DhtNode.
//!      - Finds providers for the same DHT key.
//!      - Opens a direct TCP connection to the provider's multiaddr.
//!      - Proxies the connection to their local browser.
//!
//! No relay, no central server, no bandwidth cost to ARKON.
//! The only shared infrastructure is the DHT bootstrap nodes.

use super::{DhtNode, node::{preview_key, game_key}};
use crate::server::LocalServer;
use arkon_core::error::{ArkonError, Result};
use std::path::Path;
use tokio::time::{self, Duration};
use tracing::info;

/// A DHT-based preview session. No relay dependency.
pub struct DhtProvider {
    pub peer_id:   String,
    pub local_url: String,
    pub dht_link:  String,
    ttl_secs:      u64,
    node:          DhtNode,
    server:        LocalServer,
}

impl DhtProvider {
    /// Start a DHT-based preview for `artifact_path`.
    pub async fn start(
        artifact_path: &Path,
        project_name:  &str,
        ttl_secs:      u64,
    ) -> Result<Self> {
        // 1. Start DHT node
        let node = DhtNode::start().await?;
        let peer_id_str = node.peer_id.to_string();

        // 2. Start local HTTP server
        let server = LocalServer::start(artifact_path, 0).await?;
        let local_url = server.url();

        // 3. Announce in DHT under the project preview key
        let key = preview_key(project_name);
        node.provide(&key).await?;

        // 4. Build the shareable DHT link
        //    Format: arkon://dht/<peer_id>/<project_name>
        //    Recipients parse this, query the DHT for the peer, and connect directly.
        let dht_link = format!("arkon://dht/{}/{}", peer_id_str, project_name);

        info!(
            peer_id  = %peer_id_str,
            link     = %dht_link,
            local    = %local_url,
            ttl_secs = %ttl_secs,
            "DHT preview started — no relay required"
        );

        Ok(Self { peer_id: peer_id_str, local_url, dht_link, ttl_secs, node, server })
    }

    /// Formatted TTL display.
    pub fn ttl_display(&self) -> String {
        let s = self.ttl_secs;
        if s >= 3600 && s % 3600 == 0 { format!("{}h", s / 3600) }
        else if s >= 60 { format!("{}m", s / 60) }
        else { format!("{}s", s) }
    }

    /// Block until TTL elapses, re-announcing periodically to refresh DHT records.
    pub async fn wait_for_expiry(self) {
        let reannounce_interval = Duration::from_secs(self.ttl_secs / 4);
        let mut ticker = time::interval(reannounce_interval);
        let mut elapsed = 0u64;

        loop {
            ticker.tick().await;
            elapsed += reannounce_interval.as_secs();
            if elapsed >= self.ttl_secs {
                break;
            }
            // Re-announce to keep DHT record alive (records expire after ~1h)
            let key = preview_key(""); // re-announce using stored state
            let _ = self.node.provide(&key).await;
            info!(ttl_remaining = %(self.ttl_secs - elapsed), "DHT record re-announced");
        }

        self.shutdown().await;
    }

    pub async fn shutdown(self) {
        self.node.shutdown().await;
        self.server.stop();
        info!("DHT preview session ended");
    }
}

/// Register a game server in the DHT for matchmaking discovery.
/// Other players can find it with `arkon.find_game(name, version)`.
pub struct GameServer {
    node:      DhtNode,
    game_name: String,
    version:   String,
}

impl GameServer {
    /// Announce a game server at `local_addr` in the DHT.
    pub async fn announce(
        game_name: &str,
        version:   &str,
    ) -> Result<Self> {
        let node = DhtNode::start().await?;
        let key  = game_key(game_name, version);
        node.provide(&key).await?;

        info!(
            game    = %game_name,
            version = %version,
            peer_id = %node.peer_id,
            "game server announced in DHT"
        );

        Ok(Self {
            node,
            game_name: game_name.to_string(),
            version:   version.to_string(),
        })
    }

    /// Find all announced game servers for `game_name` + `version`.
    /// Returns a list of peer IDs that are currently serving.
    pub async fn find(
        game_name: &str,
        version:   &str,
    ) -> Result<Vec<libp2p::PeerId>> {
        let node = DhtNode::start().await?;
        let key  = game_key(game_name, version);
        let mut rx = node.find_providers(&key).await?;

        let mut peers = Vec::new();
        // Collect for up to 5 seconds
        let deadline = time::Instant::now() + Duration::from_secs(5);
        loop {
            tokio::select! {
                peer = rx.recv() => {
                    match peer {
                        Some(p) => { peers.push(p); }
                        None    => break,
                    }
                }
                _ = time::sleep_until(deadline) => break,
            }
        }

        node.shutdown().await;
        info!(game = %game_name, found = %peers.len(), "game server discovery complete");
        Ok(peers)
    }

    pub async fn shutdown(self) {
        self.node.shutdown().await;
    }
}
