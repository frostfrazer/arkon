//! Relay registration client.
//!
//! The ARKON relay is a minimal open-source Go service (source in /relay/).
//! It maintains a registry of peer_id → {host, port, token}.
//! Visitors resolve a peer_id via the relay's HTTP API and are handed a
//! WebSocket proxy that tunnels to the local server.
//!
//! Protocol (relay REST API):
//!   POST /register   { peer_id, token, local_addr, ttl_secs }  → { ok }
//!   DELETE /register { peer_id, token }                         → { ok }
//!   GET  /peer/:id   (public)                                   → { ws_url }

use arkon_core::error::{ArkonError, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::time::{self, Duration};
use tracing::{debug, info, warn};

#[derive(Debug, Serialize)]
struct RegisterRequest {
    peer_id: String,
    token: String,
    local_addr: String,
    ttl_secs: u64,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    ok: bool,
    public_url: Option<String>,
    error: Option<String>,
}

pub struct RelayHandle {
    peer_id: String,
    token: String,
    relay_url: String,
    client: reqwest::Client,
    /// Cancellation token — send to stop the keepalive loop.
    cancel_tx: tokio::sync::oneshot::Sender<()>,
}

impl RelayHandle {
    /// Register with the relay and start a keepalive heartbeat.
    /// Returns the handle + the public URL visitors use.
    pub async fn register(
        relay_url: &str,
        peer_id: &str,
        local_addr: SocketAddr,
        ttl_secs: u64,
        token: &str,
    ) -> Result<(Self, String)> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ArkonError::Network(e.to_string()))?;

        let register_url = format!("{}/register", relay_url.trim_end_matches('/'));
        let body = RegisterRequest {
            peer_id: peer_id.to_string(),
            token: token.to_string(),
            local_addr: local_addr.to_string(),
            ttl_secs,
        };

        let resp = client
            .post(&register_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ArkonError::Network(format!("relay register failed: {e}")))?;

        let parsed: RegisterResponse = resp
            .json()
            .await
            .map_err(|e| ArkonError::Network(format!("relay response parse failed: {e}")))?;

        if !parsed.ok {
            return Err(ArkonError::Network(
                parsed.error.unwrap_or_else(|| "relay registration rejected".into()),
            ));
        }

        let public_url = parsed
            .public_url
            .unwrap_or_else(|| format!("{}/p/{}", relay_url.trim_end_matches('/'), peer_id));

        info!(peer_id = %peer_id, url = %public_url, "registered with relay");

        // Start keepalive — re-registers every TTL/2 seconds
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let client2   = client.clone();
            let relay_url = relay_url.to_string();
            let body2     = RegisterRequest {
                peer_id: peer_id.to_string(),
                token: token.to_string(),
                local_addr: local_addr.to_string(),
                ttl_secs,
            };
            let register_url2 = register_url.clone();
            let keepalive_interval = Duration::from_secs((ttl_secs / 2).max(30));

            tokio::spawn(async move {
                let mut ticker = time::interval(keepalive_interval);
                ticker.tick().await; // skip first immediate tick
                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            debug!("relay keepalive");
                            let _ = client2.post(&register_url2).json(&body2).send().await;
                        }
                        _ = &mut cancel_rx => {
                            debug!("relay keepalive stopped");
                            break;
                        }
                    }
                }
            });
        }

        Ok((
            Self {
                peer_id: peer_id.to_string(),
                token: token.to_string(),
                relay_url: relay_url.to_string(),
                client,
                cancel_tx,
            },
            public_url,
        ))
    }

    /// Deregister from the relay and stop the keepalive.
    pub async fn deregister(self) {
        let _ = self.cancel_tx.send(());

        let url = format!("{}/register", self.relay_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "peer_id": self.peer_id,
            "token": self.token,
        });
        let _ = self.client.delete(&url).json(&body).send().await;
        info!(peer_id = %self.peer_id, "deregistered from relay");
    }
}

/// Generate a short random token for relay session auth.
pub fn generate_token() -> String {
    use rand::Rng;
    let bytes: Vec<u8> = (0..16).map(|_| rand::thread_rng().gen::<u8>()).collect();
    hex::encode(bytes)
}
