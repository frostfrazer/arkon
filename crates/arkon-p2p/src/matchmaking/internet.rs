//! Internet-scale game server matchmaking via the Kademlia DHT.
//!
//! Game server operators run `arkon matchmaking serve --game <name> --version <ver>`.
//! Players run `arkon matchmaking find --game <name>` to get a list of live servers.
//!
//! The DHT key is content-addressed: SHA-256("arkon/game/<name>/<version>").
//! Servers re-announce every 30 minutes to keep their DHT record alive.
//! Servers that go offline stop re-announcing; records expire after ~1 hour.

use crate::dht::{DhtNode, node::game_key};
use arkon_core::error::{ArkonError, Result};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time;
use tracing::info;

/// Metadata a game server publishes alongside its DHT presence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameServerMeta {
    pub game:        String,
    pub version:     String,
    pub max_players: u32,
    pub cur_players: u32,
    pub region:      String,
    pub peer_id:     String,
}

/// Manages DHT-based game server registration and player discovery.
pub struct InternetMatchmaker {
    node:     DhtNode,
    game:     String,
    version:  String,
}

impl InternetMatchmaker {
    /// Register a game server in the DHT and keep it alive.
    /// Call `run_keepalive()` in a background task to maintain registration.
    pub async fn register(game: &str, version: &str) -> Result<Self> {
        let node = DhtNode::start().await?;
        let key  = game_key(game, version);
        node.provide(&key).await?;

        info!(
            game    = %game,
            version = %version,
            peer_id = %node.peer_id,
            "registered in DHT matchmaking"
        );

        Ok(Self {
            node,
            game:    game.to_string(),
            version: version.to_string(),
        })
    }

    /// Re-announce every 30 minutes to keep the DHT record alive.
    /// Run in a background `tokio::spawn`.
    pub async fn run_keepalive(self) {
        let interval = Duration::from_secs(30 * 60);
        let mut ticker = time::interval(interval);
        ticker.tick().await; // skip first immediate tick

        loop {
            ticker.tick().await;
            let key = game_key(&self.game, &self.version);
            match self.node.provide(&key).await {
                Ok(_)  => info!(game = %self.game, "DHT matchmaking re-announced"),
                Err(e) => tracing::warn!(error = %e, "DHT re-announce failed"),
            }
        }
    }

    /// Find live game servers for `game` + `version`.
    /// Returns peer IDs — callers use these to establish direct connections.
    pub async fn find(game: &str, version: &str) -> Result<Vec<PeerId>> {
        crate::dht::provider::GameServer::find(game, version).await
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.node.peer_id
    }

    pub async fn shutdown(self) {
        self.node.shutdown().await;
    }
}
