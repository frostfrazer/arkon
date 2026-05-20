use arkon_core::error::{ArkonError, Result};
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm,
    identify, kad,
    noise, tcp, yamux,
    swarm::SwarmEvent,
};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dnsaddr/bootstrap.arkon.sh/p2p/12D3KooWArkon1111111111111111111111111111111111111111",
    "/dnsaddr/bootstrap2.arkon.sh/p2p/12D3KooWArkon2222222222222222222222222222222222222222",
];

// ── Commands ──────────────────────────────────────────────────────────────

enum DhtCmd {
    Provide {
        key:  Vec<u8>,
        done: oneshot::Sender<Result<()>>,
    },
    FindProviders {
        key: Vec<u8>,
        tx:  mpsc::Sender<PeerId>,
    },
    Shutdown,
}

// ── Public handle ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DhtNode {
    cmd_tx:      mpsc::Sender<DhtCmd>,
    pub peer_id: PeerId,
}

impl DhtNode {
    pub async fn start() -> Result<Self> {
        let (swarm, local_peer_id) = Self::build_swarm()?;
        let (cmd_tx, cmd_rx) = mpsc::channel::<DhtCmd>(64);
        tokio::spawn(Self::event_loop(swarm, cmd_rx));
        info!(peer_id = %local_peer_id, "DHT node started");
        Ok(Self { cmd_tx, peer_id: local_peer_id })
    }

    fn build_swarm() -> Result<(Swarm<kad::Behaviour<kad::store::MemoryStore>>, PeerId)> {
        use libp2p::SwarmBuilder;

        let swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("TCP: {e}")))?
            .with_dns()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("DNS: {e}")))?
            .with_behaviour(|key| {
                let peer_id = key.public().to_peer_id();
                let store   = kad::store::MemoryStore::new(peer_id);
                let mut kademlia = kad::Behaviour::new(peer_id, store);
                kademlia.set_mode(Some(kad::Mode::Server));
                for addr_str in bootstrap_peers() {
                    if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                        if let Some(boot_peer) = peer_id_from_multiaddr(&addr) {
                            kademlia.add_address(&boot_peer, addr);
                        }
                    }
                }
                kademlia
            })
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("behaviour: {e}")))?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let peer_id = *swarm.local_peer_id();
        Ok((swarm, peer_id))
    }

    async fn event_loop(
        mut swarm: Swarm<kad::Behaviour<kad::store::MemoryStore>>,
        mut cmd_rx: mpsc::Receiver<DhtCmd>,
    ) {
        if let Err(e) = swarm.behaviour_mut().bootstrap() {
            warn!(error = %e, "DHT bootstrap failed — isolated mode");
        }

        loop {
            tokio::select! {
                event = swarm.next() => {
                    if let Some(ev) = event {
                        match ev {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                info!(addr = %address, "DHT listening");
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                                debug!(peer = %peer_id, "peer connected");
                            }
                            SwarmEvent::Behaviour(kad_ev) => {
                                debug!(event = ?kad_ev, "kademlia event");
                            }
                            _ => {}
                        }
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(DhtCmd::Provide { key, done }) => {
                            let record_key = kad::RecordKey::new(&key);
                            let result = swarm.behaviour_mut()
                                .start_providing(record_key)
                                .map(|_| ())
                                .map_err(|e| ArkonError::Other(anyhow::anyhow!("DHT provide: {e}")));
                            let _ = done.send(result);
                        }
                        Some(DhtCmd::FindProviders { key, tx }) => {
                            let record_key = kad::RecordKey::new(&key);
                            swarm.behaviour_mut().get_providers(record_key);
                            debug!("find_providers query started");
                            drop(tx);
                        }
                        Some(DhtCmd::Shutdown) | None => {
                            info!("DHT node shutting down");
                            break;
                        }
                    }
                }
            }
        }
    }

    pub async fn provide(&self, key: impl AsRef<[u8]>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DhtCmd::Provide { key: key.as_ref().to_vec(), done: tx })
            .await
            .map_err(|_| ArkonError::Other(anyhow::anyhow!("DHT node stopped")))?;
        rx.await
            .map_err(|_| ArkonError::Other(anyhow::anyhow!("DHT node stopped")))?
    }

    pub async fn find_providers(&self, key: impl AsRef<[u8]>) -> Result<mpsc::Receiver<PeerId>> {
        let (tx, rx) = mpsc::channel(32);
        self.cmd_tx
            .send(DhtCmd::FindProviders { key: key.as_ref().to_vec(), tx })
            .await
            .map_err(|_| ArkonError::Other(anyhow::anyhow!("DHT node stopped")))?;
        Ok(rx)
    }

    pub async fn shutdown(&self) {
        let _ = self.cmd_tx.send(DhtCmd::Shutdown).await;
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

pub fn preview_key(project_name: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(b"arkon/preview/");
    h.update(project_name.as_bytes());
    h.finalize().to_vec()
}

pub fn game_key(game_name: &str, version: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(b"arkon/game/");
    h.update(game_name.as_bytes());
    h.update(b"/");
    h.update(version.as_bytes());
    h.finalize().to_vec()
}

fn bootstrap_peers() -> Vec<String> {
    if let Ok(env_peers) = std::env::var("ARKON_BOOTSTRAP_PEERS") {
        return env_peers.split(',').map(str::trim).map(String::from).collect();
    }
    DEFAULT_BOOTSTRAP.iter().map(|s| s.to_string()).collect()
}

fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    use libp2p::multiaddr::Protocol;
    addr.iter().find_map(|p| {
        if let Protocol::P2p(peer_id) = p { Some(peer_id) } else { None }
    })
}
